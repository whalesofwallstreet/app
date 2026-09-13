import { render, fireEvent } from "@testing-library/react";
import { RouteGraph, type RouteOption } from "../RouteGraph";
import { ThemeProvider } from "../../ThemeContext";

// Mock ReactFlow for headless testing environment
jest.mock("@xyflow/react", () => {
  const actual = jest.requireActual("@xyflow/react");
  return {
    ...actual,
    ReactFlow: ({ nodes, onNodeClick, children }: any) => (
      <div data-testid="react-flow">
        {nodes.map((node: any) => (
          <button
            key={node.id}
            data-testid={`node-${node.id}`}
            onClick={(e) => onNodeClick && onNodeClick(e, node)}
          >
            {node.data.label}
          </button>
        ))}
        {children}
      </div>
    ),
    Background: () => <div data-testid="react-flow-background" />,
    Controls: () => <div data-testid="react-flow-controls" />,
  };
});

describe("RouteGraph Component", () => {
  const sampleRoute: RouteOption = {
    provider: "CCTP + Uniswap V3",
    path: "Ethereum (USDC) -> CCTP Bridge (50%) -> Uniswap V3 (Swap) -> Arbitrum (USDC)",
    amount_in: 1000,
    amount_out: 998.5,
    estimated_fee_usd: 0.45,
    duration_seconds: 15,
    price_impact_bps: 12,
    slippage_bps: 25,
  };

  it("renders ReactFlow graph container with parsed nodes", () => {
    const { getByTestId, getByText } = render(
      <ThemeProvider>
        <RouteGraph route={sampleRoute} />
      </ThemeProvider>
    );

    expect(getByTestId("react-flow")).toBeTruthy();
    expect(getByText("Interactive Route Path Graph")).toBeTruthy();
    expect(getByText("Ethereum (USDC)")).toBeTruthy();
    expect(getByText("CCTP Bridge (50%)")).toBeTruthy();
  });

  it("opens node metrics inspection pane when a node is clicked", () => {
    const { getByTestId, getByText } = render(
      <ThemeProvider>
        <RouteGraph route={sampleRoute} />
      </ThemeProvider>
    );

    const bridgeNodeBtn = getByTestId("node-node_1");
    fireEvent.click(bridgeNodeBtn);

    expect(getByText("CCTP Bridge (50%)")).toBeTruthy();
    expect(getByText("25 bps")).toBeTruthy();
  });
});
