import { describe, it, expect, vi, beforeEach } from "vitest";
import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import type { CustomerDevenv } from "@/lib/customer-types";

// Mock Clerk Auth
vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ getToken: () => Promise.resolve("mock_tok") }),
}));

// Mock useCustomerClient
const mockListDevenvs = vi.fn();
const mockCreateDevenv = vi.fn();
const mockStopDevenv = vi.fn();
const mockClient = {
  listDevenvs: mockListDevenvs,
  createDevenv: mockCreateDevenv,
  stopDevenv: mockStopDevenv,
};

vi.mock("@/lib/use-customer-client", () => ({
  useCustomerClient: () => mockClient,
}));

import { DevenvClient } from "@/components/customer/DevenvClient";

describe("DevenvClient UI Component", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders empty state when no devenv is active", async () => {
    mockListDevenvs.mockResolvedValueOnce({ devenvs: [] });

    render(<DevenvClient />);

    await waitFor(() => {
      expect(screen.getByText("No active DevEnv sessions")).toBeInTheDocument();
      expect(screen.getByText("Launch DevEnv")).toBeInTheDocument();
    });
  });

  it("renders active running session with hardware tier and action buttons", async () => {
    const mockDev: CustomerDevenv = {
      devenv_id: "dev-tenant-123",
      status: "running",
      workspace_name: "hermes-agent-workspace",
      profile_name: "prod-browser",
      tier: "power-8",
      created_at: Date.now() - 60000,
      started_at: Date.now() - 60000,
      ports: [6080, 7681, 8080],
    };

    mockListDevenvs.mockResolvedValueOnce({ devenvs: [mockDev] });

    render(<DevenvClient />);

    await waitFor(() => {
      expect(screen.getByText("hermes-agent-workspace")).toBeInTheDocument();
      expect(screen.getByText("Running")).toBeInTheDocument();
      expect(screen.getByText("Power (8 vCPU, 16 GB)")).toBeInTheDocument();
      expect(screen.getByText("VS Code")).toBeInTheDocument();
      expect(screen.getByText("Terminal")).toBeInTheDocument();
      expect(screen.getByText("Desktop (noVNC)")).toBeInTheDocument();
      expect(screen.getByText("Stop")).toBeInTheDocument();
    });
  });
});
