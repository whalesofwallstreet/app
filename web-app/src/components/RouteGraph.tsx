import React, { useState, useMemo } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  type Node,
  type Edge,
  Position,
  MarkerType,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useTheme } from "../ThemeContext";
import { Ionicons } from "./Ionicons";

export interface RouteOption {
  provider: string;
  path: string;
  amount_in: number;
  amount_out: number;
  estimated_fee_usd: number;
  duration_seconds: number;
  price_impact_bps: number;
  slippage_bps: number;
  execution_payload?: string;
}

export interface RouteGraphProps {
  route?: RouteOption;
}

interface NodeDetails {
  id: string;
  title: string;
  type: string;
  feeUsd: number;
  slippageBps: number;
  durationSec: number;
  amountIn: number;
  amountOut: number;
  details: string;
}

export const RouteGraph: React.FC<RouteGraphProps> = ({ route }) => {
  const { c, theme } = useTheme();
  const [selectedNode, setSelectedNode] = useState<NodeDetails | null>(null);

  const defaultRoute: RouteOption = useMemo(
    () =>
      route || {
        provider: "CCTP + Uniswap V3",
        path: "Ethereum (USDC) -> CCTP Bridge (50%) -> Uniswap V3 (Swap) -> Arbitrum (USDC)",
        amount_in: 1000,
        amount_out: 998.5,
        estimated_fee_usd: 0.45,
        duration_seconds: 15,
        price_impact_bps: 12,
        slippage_bps: 25,
        execution_payload: "0x32f7e5...c487",
      },
    [route]
  );

  const { nodes, edges } = useMemo(() => {
    const rawHops = defaultRoute.path.split("->").map((s) => s.trim());
    const generatedNodes: Node[] = [];
    const generatedEdges: Edge[] = [];

    const isDark = theme === "dark";
    const cardBg = isDark ? "#161b22" : "#ffffff";
    const textColor = isDark ? "#f0f6fc" : "#0d1117";
    const borderColor = isDark ? "#30363d" : "#e1e4e8";

    rawHops.forEach((hop, idx) => {
      const isFirst = idx === 0;
      const isLast = idx === rawHops.length - 1;
      const isBridge = hop.toLowerCase().includes("bridge") || hop.toLowerCase().includes("cctp");
      const isDex = hop.toLowerCase().includes("swap") || hop.toLowerCase().includes("uniswap") || hop.toLowerCase().includes("dex");

      const nodeType = isFirst || isLast ? "CHAIN" : isBridge ? "BRIDGE" : isDex ? "DEX" : "PROTOCOL";

      generatedNodes.push({
        id: `node_${idx}`,
        position: { x: idx * 220 + 40, y: 120 },
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
        data: {
          label: hop,
          nodeType,
          feeUsd: defaultRoute.estimated_fee_usd / Math.max(1, rawHops.length - 1),
          slippageBps: defaultRoute.slippage_bps,
          durationSec: defaultRoute.duration_seconds,
          amountIn: isFirst ? defaultRoute.amount_in : defaultRoute.amount_in * 0.999,
          amountOut: isLast ? defaultRoute.amount_out : defaultRoute.amount_in * 0.999,
        },
        style: {
          background: cardBg,
          color: textColor,
          border: `1.5px solid ${isBridge || isDex ? c.primary : borderColor}`,
          borderRadius: "16px",
          padding: "12px 16px",
          fontWeight: 700,
          fontSize: "13px",
          boxShadow: "0 8px 16px rgba(0,0,0,0.12)",
          width: 170,
          textAlign: "center",
        },
      });

      if (idx > 0) {
        generatedEdges.push({
          id: `edge_${idx - 1}_${idx}`,
          source: `node_${idx - 1}`,
          target: `node_${idx}`,
          animated: true,
          label: idx === 1 ? `50% Split • ${defaultRoute.slippage_bps} bps` : `${defaultRoute.slippage_bps} bps`,
          style: { stroke: c.primary, strokeWidth: 2 },
          labelStyle: { fill: c.mutedForeground, fontWeight: 600, fontSize: 10 },
          labelBgStyle: { fill: cardBg, fillOpacity: 0.85, rx: 6, ry: 6 },
          markerEnd: { type: MarkerType.ArrowClosed, color: c.primary },
        });
      }
    });

    return { nodes: generatedNodes, edges: generatedEdges };
  }, [defaultRoute, theme, c]);

  const onNodeClick = (_: React.MouseEvent, node: Node) => {
    const data = node.data as any;
    setSelectedNode({
      id: node.id,
      title: data.label,
      type: data.nodeType,
      feeUsd: data.feeUsd,
      slippageBps: data.slippageBps,
      durationSec: data.durationSec,
      amountIn: data.amountIn,
      amountOut: data.amountOut,
      details: defaultRoute.execution_payload || "Execution payload verified",
    });
  };

  return (
    <div className="route-graph-container" style={{ width: "100%", margin: "16px 0" }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: "12px",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <Ionicons name="git-network-outline" size={18} color={c.primary} />
          <span style={{ fontSize: "14px", fontWeight: "700", color: c.foreground }}>
            Interactive Route Path Graph
          </span>
        </div>
        <span style={{ fontSize: "12px", fontWeight: "600", color: c.mutedForeground }}>
          Click nodes for hop metrics
        </span>
      </div>

      <div
        style={{
          height: "300px",
          width: "100%",
          borderRadius: "24px",
          overflow: "hidden",
          border: `1px solid ${c.border}80`,
          backgroundColor: c.card,
          position: "relative",
        }}
      >
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodeClick={onNodeClick}
          fitView
          attributionPosition="bottom-right"
        >
          <Background color={c.border} gap={16} size={1} />
          <Controls style={{ borderRadius: "12px", overflow: "hidden" }} />
        </ReactFlow>

        {/* Selected Node Details Pane */}
        {selectedNode && (
          <div
            className="fade-in"
            style={{
              position: "absolute",
              bottom: "12px",
              left: "12px",
              right: "12px",
              backgroundColor: c.secondary,
              borderRadius: "16px",
              padding: "14px 18px",
              border: `1px solid ${c.primary}40`,
              boxShadow: "0 8px 24px rgba(0,0,0,0.2)",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              zIndex: 10,
            }}
          >
            <div>
              <div style={{ fontSize: "14px", fontWeight: "800", color: c.foreground }}>
                {selectedNode.title}
              </div>
              <div
                style={{
                  display: "flex",
                  gap: "12px",
                  marginTop: "4px",
                  fontSize: "12px",
                  color: c.mutedForeground,
                }}
              >
                <span>Type: <strong style={{ color: c.foreground }}>{selectedNode.type}</strong></span>
                <span>Est. Gas: <strong style={{ color: c.success }}>${selectedNode.feeUsd.toFixed(2)}</strong></span>
                <span>Slippage: <strong style={{ color: c.primary }}>{selectedNode.slippageBps} bps</strong></span>
                <span>Duration: <strong style={{ color: c.foreground }}>{selectedNode.durationSec}s</strong></span>
              </div>
            </div>
            <button
              onClick={() => setSelectedNode(null)}
              style={{
                background: "transparent",
                border: "none",
                color: c.mutedForeground,
                cursor: "pointer",
                padding: "4px",
              }}
            >
              <Ionicons name="close" size={18} color={c.foreground} />
            </button>
          </div>
        )}
      </div>
    </div>
  );
};
