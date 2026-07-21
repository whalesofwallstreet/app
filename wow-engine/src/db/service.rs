use crate::db::models::RouteExecutionInput;
use crate::db::operations::{
    AnchorTransactionRepo, RouteExecutionRepo, UserHistoryRepo, UserQuotaRepo,
};
use crate::db::Database;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ExecuteRouteResult {
    pub route_id: Uuid,
    pub anchor_transaction_id: Option<Uuid>,
    pub success: bool,
}

pub struct RouteExecutionService;

impl RouteExecutionService {
    pub async fn execute_route_with_quota(
        db: &Database,
        route_input: RouteExecutionInput,
        anchor_domain: Option<&str>,
        anchor_transaction_id: Option<&str>,
    ) -> Result<ExecuteRouteResult, Box<dyn std::error::Error>> {
        let mut tx = db.begin().await?;

        // 1. Create the route execution record with initial history
        let route = RouteExecutionRepo::create_with_history(
            &mut tx,
            route_input.clone(),
            "route_created",
            "Route execution initiated",
        )
        .await?;

        // 2. Get or create user quota
        let quota = UserQuotaRepo::get_or_create(&mut tx, route_input.user_id).await?;

        // 3. Check if user has remaining quota
        let usage_amount = route_input.estimated_fee_usd;
        if quota.used_today_usd + usage_amount > quota.daily_limit_usd {
            // Quota exceeded - ENTIRE transaction rolls back automatically
            return Err(format!(
                "Daily quota exceeded: {}/{} USD",
                quota.used_today_usd + usage_amount,
                quota.daily_limit_usd
            )
            .into());
        }

        // 4. Update quota with the new usage
        UserQuotaRepo::update_usage(&mut tx, route_input.user_id, usage_amount).await?;

        // 5. Log the quota update
        UserHistoryRepo::create(
            &mut tx,
            route_input.user_id,
            route.id,
            "quota_updated",
            &format!("Quota used: {} USD", usage_amount),
        )
        .await?;

        let mut anchor_tx_id = None;

        // 6. If anchor transaction is provided, create it
        if let (Some(domain), Some(tx_id)) = (anchor_domain, anchor_transaction_id) {
            let anchor_tx = AnchorTransactionRepo::create(
                &mut tx,
                route_input.user_id,
                route.id,
                domain,
                tx_id,
                &format!("https://{}/interactive", domain),
            )
            .await?;
            anchor_tx_id = Some(anchor_tx.id);

            UserHistoryRepo::create(
                &mut tx,
                route_input.user_id,
                route.id,
                "anchor_transaction_created",
                &format!("Anchor domain: {}, Transaction: {}", domain, tx_id),
            )
            .await?;
        }

        // 7. Update route status to "executed"
        RouteExecutionRepo::update_status(&mut tx, route.id, "executed").await?;

        // 8. Log final status
        UserHistoryRepo::create(
            &mut tx,
            route_input.user_id,
            route.id,
            "route_executed",
            "Route execution completed successfully",
        )
        .await?;

        // COMMIT: All operations succeed together or entire transaction rolls back
        tx.commit().await?;

        Ok(ExecuteRouteResult {
            route_id: route.id,
            anchor_transaction_id: anchor_tx_id,
            success: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_route_result_creation() {
        let route_id = Uuid::new_v7();
        let result = ExecuteRouteResult {
            route_id,
            anchor_transaction_id: Some(Uuid::new_v7()),
            success: true,
        };

        assert!(result.success);
        assert!(result.anchor_transaction_id.is_some());
    }
}
