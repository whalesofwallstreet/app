import { useState } from "react";
import styled from "styled-components";
import { useTheme } from "../ThemeContext";
import { useAppNavigation } from "../context/NavigationContext";
import { Ionicons } from "../components/Ionicons";
import { fonts, shadows } from "../theme";

const Container = styled.div`
  max-width: 700px;
  margin: 0 auto;
`;

const HeaderContainer = styled.div`
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 32px;
`;

const BackButton = styled.button`
  padding: 8px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  cursor: pointer;
  transition: opacity 0.2s;
  
  &:hover {
    opacity: 0.8;
  }
`;

const Title = styled.span`
  font-size: 22px;
  font-weight: 800;
  color: ${({ theme }) => theme.colors.foreground};
  font-family: ${fonts.display};
  letter-spacing: -0.5px;
`;

const SectionLabel = styled.div`
  font-size: 12px;
  font-weight: 700;
  color: ${({ theme }) => theme.colors.mutedForeground};
  letter-spacing: 1.5px;
  margin-bottom: 16px;
`;

const AssetCard = styled.div`
  display: flex;
  align-items: center;
  gap: 20px;
  padding: 20px 24px;
  border-radius: 24px;
  background-color: ${({ theme }) => theme.colors.card};
  border: 1px solid ${({ theme }) => theme.colors.border}50;
  margin-bottom: 16px;
  cursor: pointer;
  box-shadow: ${shadows.card};
  transition: transform 0.2s, box-shadow 0.2s;

  &:hover {
    transform: translateY(-2px);
    box-shadow: ${shadows.elevated};
  }
`;

const IconWrapper = styled.div`
  width: 52px;
  height: 52px;
  border-radius: 18px;
  background-color: ${({ theme }) => theme.colors.primary}12;
  display: flex;
  align-items: center;
  justify-content: center;
`;

const AssetInfo = styled.div`
  flex: 1;
`;

const AssetLabel = styled.div`
  font-size: 16px;
  font-weight: 700;
  color: ${({ theme }) => theme.colors.foreground};
`;

const AssetDesc = styled.div`
  font-size: 13px;
  color: ${({ theme }) => theme.colors.mutedForeground};
`;

const TagBadge = styled.div`
  padding: 8px 16px;
  border-radius: 14px;
  background-color: ${({ theme }) => theme.colors.primary}12;
  font-size: 13px;
  font-weight: 700;
  color: ${({ theme }) => theme.colors.primary};
`;

const Overlay = styled.div`
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background-color: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  backdrop-filter: blur(4px);
`;

const Modal = styled.div`
  background-color: ${({ theme }) => theme.colors.background};
  padding: 24px;
  border-radius: 24px;
  width: 90%;
  max-width: 400px;
  box-shadow: ${shadows.elevated};
  border: 1px solid ${({ theme }) => theme.colors.border};
`;

const ModalHeader = styled.div`
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 20px;
`;

const ModalTitle = styled.h3`
  margin: 0;
  font-size: 18px;
  color: ${({ theme }) => theme.colors.foreground};
`;

const ModalDesc = styled.p`
  margin: 4px 0 0;
  font-size: 14px;
  color: ${({ theme }) => theme.colors.mutedForeground};
`;

const ModalBodyText = styled.p`
  color: ${({ theme }) => theme.colors.foreground};
  font-size: 15px;
  line-height: 1.5;
  margin-bottom: 24px;
`;

const ButtonRow = styled.div`
  display: flex;
  gap: 12px;
`;

const CancelButton = styled.button`
  flex: 1;
  padding: 14px;
  border-radius: 16px;
  border: 1px solid ${({ theme }) => theme.colors.border};
  background-color: transparent;
  color: ${({ theme }) => theme.colors.foreground};
  font-size: 15px;
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.2s;

  &:hover {
    opacity: 0.7;
  }
`;

const ConfirmButton = styled.button`
  flex: 1;
  padding: 14px;
  border-radius: 16px;
  border: none;
  background-color: ${({ theme }) => theme.colors.primary};
  color: ${({ theme }) => theme.colors.background};
  font-size: 15px;
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.2s;

  &:hover {
    opacity: 0.9;
  }
`;

export const InvestScreen = () => {
  const { c } = useTheme();
  const { goBack } = useAppNavigation();
  const [selectedAsset, setSelectedAsset] = useState<any>(null);

  const investOptions = [
    { icon: "bar-chart-outline", label: "Stocks & ETFs", desc: "Invest in global markets", tag: "Popular" },
    { icon: "logo-bitcoin", label: "Crypto Assets", desc: "Buy and hold digital assets", tag: "Volatile" },
    { icon: "leaf-outline", label: "Green Energy Funds", desc: "Sustainable & ESG portfolios", tag: "New Portfolio" },
  ];

  return (
    <Container className="fade-in">
      <HeaderContainer>
        <BackButton onClick={goBack}>
          <Ionicons name="chevron-back" size={24} color={c.foreground} />
        </BackButton>
        <Title>Whales Investment Platform</Title>
      </HeaderContainer>

      <SectionLabel>CHOOSE AN ASSET CLASS</SectionLabel>

      {investOptions.map((opt) => (
        <AssetCard key={opt.label} onClick={() => setSelectedAsset(opt)}>
          <IconWrapper>
            <Ionicons name={opt.icon} size={24} color={c.primary} />
          </IconWrapper>
          <AssetInfo>
            <AssetLabel>{opt.label}</AssetLabel>
            <AssetDesc>{opt.desc}</AssetDesc>
          </AssetInfo>
          <TagBadge>{opt.tag}</TagBadge>
        </AssetCard>
      ))}

      {selectedAsset && (
        <Overlay>
          <Modal className="fade-in-up">
            <ModalHeader>
              <IconWrapper>
                <Ionicons name={selectedAsset.icon} size={24} color={c.primary} />
              </IconWrapper>
              <div>
                <ModalTitle>{selectedAsset.label}</ModalTitle>
                <ModalDesc>{selectedAsset.desc}</ModalDesc>
              </div>
            </ModalHeader>
            
            <ModalBodyText>
              You are about to subscribe to the {selectedAsset.label} portfolio. Please review your selection before continuing.
            </ModalBodyText>

            <ButtonRow>
              <CancelButton onClick={() => setSelectedAsset(null)}>Cancel</CancelButton>
              <ConfirmButton
                onClick={() => {
                  alert(`Successfully subscribed to ${selectedAsset.label}!`);
                  setSelectedAsset(null);
                }}
              >
                Confirm
              </ConfirmButton>
            </ButtonRow>
          </Modal>
        </Overlay>
      )}
    </Container>
  );
};
