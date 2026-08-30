// apps/admin-ui/src/components/customer/DevenvClient.tsx
// CoreLink DevEnv (Elastic Sandboxes) Dashboard UI (WP-09)
"use client";

import React from "react";
import { useCustomerClient } from "@/lib/use-customer-client";
import type { CustomerDevenv, DevenvTier } from "@/lib/customer-types";
import {
  Badge,
  Button,
  Callout,
  Card,
  ConfirmDialog,
  EmptyState,
  Field,
  HelpPopover,
  InlineError,
  Input,
  Modal,
  Skeleton,
  ToastProvider,
  useToast,
} from "@/components/ui/linear";

const TIER_OPTIONS: Array<{ value: DevenvTier; label: string; vcpu: number; ram: string }> = [
  { value: "standard-2", label: "Standard (2 vCPU, 4 GB)", vcpu: 2, ram: "4 GB" },
  { value: "standard-4", label: "Standard (4 vCPU, 8 GB)", vcpu: 4, ram: "8 GB" },
  { value: "power-8",    label: "Power (8 vCPU, 16 GB)",   vcpu: 8, ram: "16 GB" },
  { value: "ultra-16",   label: "Ultra (16 vCPU, 32 GB)",  vcpu: 16, ram: "32 GB" },
];

function StatusBadge({ status }: { status: CustomerDevenv["status"] }) {
  switch (status) {
    case "running":
      return <Badge tone="success" dot>Running</Badge>;
    case "starting":
      return <Badge tone="warn" dot>Starting (Waking up...)</Badge>;
    case "stopping":
      return <Badge tone="neutral">Stopping</Badge>;
    case "errored":
      return <Badge tone="danger">Errored</Badge>;
    case "stopped":
    default:
      return <Badge tone="neutral">Stopped</Badge>;
  }
}

function humanizeUptime(ms: number | null): string {
  if (!ms || ms <= 0) return "—";
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  if (hours > 0) return `${hours}h ${minutes % 60}m`;
  if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
  return `${seconds}s`;
}

function DevenvInner(): React.ReactElement {
  const client = useCustomerClient();
  const { toast } = useToast();

  const [devenvs, setDevenvs] = React.useState<CustomerDevenv[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<unknown | null>(null);

  const [createModalOpen, setCreateModalOpen] = React.useState(false);
  const [workspaceName, setWorkspaceName] = React.useState("");
  const [profileName, setProfileName] = React.useState("default");
  const [selectedTier, setSelectedTier] = React.useState<DevenvTier>("standard-4");
  const [clwToken, setClwToken] = React.useState("");
  const [creating, setCreating] = React.useState(false);

  const [stopTarget, setStopTarget] = React.useState<CustomerDevenv | null>(null);
  const [stopping, setStopping] = React.useState(false);

  const reload = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await client.listDevenvs();
      setDevenvs(res?.devenvs ?? []);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [client]);

  React.useEffect(() => {
    reload();
  }, [reload]);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!workspaceName.trim()) return;
    setCreating(true);
    try {
      await client.createDevenv({
        workspace_name: workspaceName.trim(),
        profile_name: profileName.trim() || "default",
        tier: selectedTier,
        clw_token: clwToken.trim(),
      });
      toast({ title: "DevEnv session starting", tone: "success" });
      setCreateModalOpen(false);
      setWorkspaceName("");
      setProfileName("default");
      await reload();
    } catch (err: any) {
      toast({ title: `Failed to launch: ${err.message ?? "Unknown error"}`, tone: "danger" });
    } finally {
      setCreating(false);
    }
  };

  const handleStop = async () => {
    if (!stopTarget) return;
    setStopping(true);
    try {
      await client.stopDevenv(stopTarget.devenv_id);
      toast({ title: "DevEnv stopped & snapshotted", tone: "neutral" });
      setStopTarget(null);
      await reload();
    } catch (err: any) {
      toast({ title: `Failed to stop: ${err.message ?? "Unknown error"}`, tone: "danger" });
    } finally {
      setStopping(false);
    }
  };

  if (loading && devenvs.length === 0) {
    return (
      <div className="space-y-4">
        <Skeleton rows={3} />
      </div>
    );
  }

  if (error) {
    return <InlineError error={error} onRetry={reload} />;
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold tracking-tight text-neutral-100 flex items-center gap-2">
            Development Environments
            <HelpPopover label="About DevEnv">
              Persistent cloud development sandboxes with browser profiles and code-server. Scales to zero on idle.
            </HelpPopover>
          </h1>
          <p className="text-sm text-neutral-400">
            Isolated edge microVMs with stateful Content-Addressable Storage (CAS) snapshots.
          </p>
        </div>
        <Button variant="primary" onClick={() => setCreateModalOpen(true)}>
          Launch DevEnv
        </Button>
      </div>

      {devenvs.length === 0 ? (
        <EmptyState
          title="No active DevEnv sessions"
          body="Launch a cloud development sandbox to run browser automations, code editing, and background agents."
          cta={
            <Button variant="primary" onClick={() => setCreateModalOpen(true)}>
              Launch Sandbox
            </Button>
          }
        />
      ) : (
        <div className="grid gap-4">
          {devenvs.map((dev) => (
            <Card key={dev.devenv_id}>
              <div className="p-6 space-y-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <span className="font-mono text-base font-medium text-neutral-200">{dev.workspace_name}</span>
                    <StatusBadge status={dev.status} />
                    <Badge tone="neutral">{dev.tier}</Badge>
                  </div>
                  <div className="flex items-center gap-2">
                    {dev.status === "running" && (
                      <>
                        <Button variant="ghost" size="sm" onClick={() => window.open(`http://localhost:8080`, "_blank")}>
                          VS Code
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => window.open(`http://localhost:7681`, "_blank")}>
                          Terminal
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => window.open(`http://localhost:6080`, "_blank")}>
                          Desktop (noVNC)
                        </Button>
                      </>
                    )}
                    {dev.status !== "stopped" && dev.status !== "stopping" && (
                      <Button variant="danger" size="sm" onClick={() => setStopTarget(dev)}>
                        Stop
                      </Button>
                    )}
                  </div>
                </div>

                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs text-neutral-400 border-t border-neutral-800 pt-4">
                  <div>
                    <div className="text-neutral-500">Browser Profile</div>
                    <div className="font-mono text-neutral-300">{dev.profile_name}</div>
                  </div>
                  <div>
                    <div className="text-neutral-500">Hardware Allocation</div>
                    <div className="text-neutral-300">{TIER_OPTIONS.find((t) => t.value === dev.tier)?.label ?? dev.tier}</div>
                  </div>
                  <div>
                    <div className="text-neutral-500">Uptime</div>
                    <div className="text-neutral-300">{humanizeUptime(dev.started_at ? Date.now() - dev.started_at : null)}</div>
                  </div>
                  <div>
                    <div className="text-neutral-500">Session ID</div>
                    <div className="font-mono text-neutral-400 truncate">{dev.devenv_id}</div>
                  </div>
                </div>
              </div>
            </Card>
          ))}
        </div>
      )}

      {/* Launch Modal */}
      <Modal open={createModalOpen} onClose={() => setCreateModalOpen(false)} title="Launch Cloud DevEnv">
        <form onSubmit={handleCreate} className="space-y-4">
          <Field label="Workspace Name">
            <Input
              value={workspaceName}
              onChange={(e) => setWorkspaceName(e.target.value)}
              placeholder="e.g. my-agent-sandbox"
            />
          </Field>

          <Field label="Browser Profile">
            <Input
              value={profileName}
              onChange={(e) => setProfileName(e.target.value)}
              placeholder="default"
            />
          </Field>

          <Field label="Hardware Sizing (Scale-to-Infinity)">
            <select
              value={selectedTier}
              onChange={(e) => setSelectedTier(e.target.value as DevenvTier)}
              className="w-full bg-neutral-900 border border-neutral-800 rounded px-3 py-2 text-sm text-neutral-200"
            >
              {TIER_OPTIONS.map((t) => (
                <option key={t.value} value={t.value}>
                  {t.label}
                </option>
              ))}
            </select>
          </Field>

          <Field label="CoreLink Access Token (PAT)">
            <Input
              type="password"
              value={clwToken}
              onChange={(e) => setClwToken(e.target.value)}
              placeholder="cl_pat_..."
            />
          </Field>

          <Callout tone="info">
            The environment boots in ~15-20s. Unused sessions automatically hibernate to zero vCPU cost after 30 minutes of inactivity.
          </Callout>

          <div className="flex justify-end gap-2 pt-2">
            <Button variant="ghost" type="button" onClick={() => setCreateModalOpen(false)} disabled={creating}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" loading={creating}>
              Launch Environment
            </Button>
          </div>
        </form>
      </Modal>

      {/* Confirm Stop Dialog */}
      <ConfirmDialog
        open={stopTarget !== null}
        onClose={() => setStopTarget(null)}
        title="Stop DevEnv Session?"
        body={`Are you sure you want to stop '${stopTarget?.workspace_name}'? The workspace and browser profile will be snapshotted to R2.`}
        confirmLabel="Stop & Snapshot"
        danger
        onConfirm={handleStop}
      />
    </div>
  );
}

export function DevenvClient(): React.ReactElement {
  return (
    <ToastProvider>
      <DevenvInner />
    </ToastProvider>
  );
}
