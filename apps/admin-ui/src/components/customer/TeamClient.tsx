// Customer-side team management — members (roles explained), invite, and remove.
//
// W6 (customer-dashboard build wave). Kit-only: every surface is a
// `@/components/ui/linear` primitive. Each role carries a HelpPopover explaining
// its permissions (the audit found roles were unexplained). Remove wires the
// previously-UI-less backend DELETE …/team/:id behind a danger ConfirmDialog that
// warns it revokes the member's tokens; every mutation fires a Toast. Data-truth:
// list/invite/remove are [live]; prod may return only a synthesized Owner ([stub]
// until BE-6) — the single-owner case shows a friendly invite hint, never a blank.

"use client";

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerTeamMember } from "@/lib/customer-types";
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
  Select,
  Skeleton,
  ToastProvider,
  useToast,
} from "@/components/ui/linear";

type Role = CustomerTeamMember["role"];
type Status = CustomerTeamMember["status"];

const ROLES: Role[] = ["Owner", "Admin", "Developer", "Viewer"];

/** Plain-language description of what each RBAC role can do (the audit found
 *  roles were unexplained). Surfaced via a HelpPopover on every role badge. */
const ROLE_HELP: Record<Role, string> = {
  Owner: "Full control — manage members, tokens, billing, and delete the account.",
  Admin: "Manage members and API tokens. Cannot change billing or delete the account.",
  Developer: "Read and write the cache and mint their own tokens. No member management.",
  Viewer: "Read-only access to usage and the audit log. Cannot change anything.",
};

const STATUS_TONE: Record<Status, "success" | "warn" | "neutral"> = {
  active: "success",
  invited: "warn",
  suspended: "neutral",
};

const STATUS_LABEL: Record<Status, string> = {
  active: "Active",
  invited: "Invited",
  suspended: "Suspended",
};

function formatJoined(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

function TeamInner(): React.ReactElement {
  const { getToken } = useAuth();
  const { toast } = useToast();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);

  const [members, setMembers] = React.useState<CustomerTeamMember[]>([]);
  const [email, setEmail] = React.useState("");
  const [role, setRole] = React.useState<Role>("Developer");
  const [inviting, setInviting] = React.useState(false);
  const [loading, setLoading] = React.useState(true);
  const [err, setErr] = React.useState<unknown | null>(null);
  const [lastInvited, setLastInvited] = React.useState<string | null>(null);
  const [removeTarget, setRemoveTarget] = React.useState<CustomerTeamMember | null>(null);

  const reload = React.useCallback(async () => {
    setLoading(true);
    setErr(null);
    try {
      const r = await client.listTeam();
      setMembers(r.members);
    } catch (e) {
      setErr(e);
    } finally {
      setLoading(false);
    }
  }, [client]);

  React.useEffect(() => {
    void reload();
  }, [reload]);

  async function onInvite(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    if (inviting) return;
    setErr(null);
    setInviting(true);
    try {
      const m = await client.inviteTeam({ email, role });
      setLastInvited(m.email);
      setEmail("");
      toast({ title: `Invite sent to ${m.email}`, tone: "success" });
      await reload();
    } catch (ex) {
      setErr(ex);
      toast({ title: "Couldn't send the invite", tone: "danger" });
    } finally {
      setInviting(false);
    }
  }

  async function onConfirmRemove(): Promise<void> {
    const target = removeTarget;
    if (!target) return;
    try {
      const res = await client.removeTeamMember(target.user_id);
      const n = res.revoked_pats;
      toast({
        title: `Removed ${target.email} — revoked ${n} ${n === 1 ? "token" : "tokens"}`,
        tone: "success",
      });
      await reload();
    } catch (ex) {
      setErr(ex);
      toast({ title: "Couldn't remove the member", tone: "danger" });
    } finally {
      setRemoveTarget(null);
    }
  }

  // Active/suspended seats vs. still-pending invitations are visually distinct
  // groups (the audit found invited members rendered inline with no management).
  const activeMembers = members.filter((m) => m.status !== "invited");
  const pendingInvites = members.filter((m) => m.status === "invited");
  const ownerCount = members.filter((m) => m.role === "Owner").length;
  const singleOwnerOnly =
    activeMembers.length === 1 && ownerCount === 1 && pendingInvites.length === 0;
  // Seat cap is NOT on the wire — listTeam returns only the member array (no
  // plan/seat-limit field), so we honestly count members instead of faking an
  // "X of Y seats". Close BE-6 to surface a real cap.
  const seatsLabel = `${activeMembers.length} ${activeMembers.length === 1 ? "member" : "members"}`;
  const pendingLabel = `${pendingInvites.length} ${pendingInvites.length === 1 ? "invitation" : "invitations"}`;

  return (
    <div data-testid="team-shell">
      <Card
        title="Invite a teammate"
        meta="They'll get an email invitation to join this tenant."
      >
        <form data-testid="team-invite" onSubmit={onInvite}>
          <Field label="Email" htmlFor="team-invite-email-input">
            <Input
              id="team-invite-email-input"
              type="email"
              data-testid="team-invite-email"
              required
              placeholder="teammate@company.com"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
          </Field>
          <Field
            label="Role"
            htmlFor="team-invite-role-select"
            help="What this teammate can do. Each role's full permissions are explained on its badge in the members list below. Roles are set at invite time and can't be changed later — remove and re-invite to change a role."
          >
            <Select
              id="team-invite-role-select"
              data-testid="team-invite-role"
              value={role}
              onChange={(e) => setRole(e.target.value as Role)}
            >
              {ROLES.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </Select>
          </Field>
          <Button type="submit" data-testid="team-invite-submit" loading={inviting}>
            Send invite
          </Button>
        </form>

        {lastInvited != null ? (
          <Callout tone="info">
            <span data-testid="team-invite-success">
              Invited {lastInvited}. We&apos;ve emailed them an invitation — they&apos;ll show
              under &quot;Pending invitations&quot; until they accept.
            </span>
          </Callout>
        ) : null}
      </Card>

      <Card title="Members" meta={seatsLabel} className="lin-mt-lg">
        {loading ? (
          <div data-testid="team-loading" aria-busy="true" aria-label="Loading members">
            <Skeleton rows={3} />
          </div>
        ) : err != null ? (
          <div data-testid="team-error">
            <InlineError error={err} onRetry={() => void reload()} />
          </div>
        ) : members.length === 0 ? (
          <EmptyState
            title="No members yet"
            body="Invite your team to share cache warmth and manage tokens together."
          />
        ) : (
          <>
            {singleOwnerOnly ? (
              <Callout tone="info">
                It&apos;s just you so far. Invite your team to share cache warmth and manage
                tokens together.
              </Callout>
            ) : null}
            <table className="lin-table" data-testid="team-list">
              <thead>
                <tr>
                  <th>Email</th>
                  <th>Role</th>
                  <th>Status</th>
                  <th>Joined</th>
                  <th aria-label="Actions" />
                </tr>
              </thead>
              <tbody>
                {activeMembers.map((m) => {
                  const isOwner = m.role === "Owner";
                  return (
                    <tr key={m.user_id} data-testid={`team-row-${m.user_id}`}>
                      <td>{m.email}</td>
                      <td data-testid={`team-role-${m.user_id}`}>
                        <Badge tone="neutral">{m.role}</Badge>{" "}
                        <HelpPopover label={`What can a ${m.role} do?`}>
                          {ROLE_HELP[m.role]}
                        </HelpPopover>
                      </td>
                      <td>
                        <Badge tone={STATUS_TONE[m.status]} dot>
                          {STATUS_LABEL[m.status]}
                        </Badge>
                      </td>
                      <td>{formatJoined(m.joined_at)}</td>
                      <td>
                        {isOwner ? null : (
                          <Button
                            variant="danger"
                            size="sm"
                            data-testid={`team-remove-${m.user_id}`}
                            onClick={() => setRemoveTarget(m)}
                          >
                            Remove
                          </Button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            <p className="lin-t3 lin-mt">
              Roles are set when you invite someone and can&apos;t be changed here yet — to change a
              role, remove the member and re-invite them.
            </p>
          </>
        )}
      </Card>

      {!loading && err == null && pendingInvites.length > 0 ? (
        <Card title="Pending invitations" meta={pendingLabel} className="lin-mt">
          <table className="lin-table" data-testid="team-pending-list">
            <thead>
              <tr>
                <th>Email</th>
                <th>Role</th>
                <th>Status</th>
                <th>Invited</th>
                <th aria-label="Actions" />
              </tr>
            </thead>
            <tbody>
              {pendingInvites.map((m) => (
                <tr key={m.user_id} data-testid={`team-row-${m.user_id}`}>
                  <td>{m.email}</td>
                  <td>
                    <Badge tone="neutral">{m.role}</Badge>{" "}
                    <HelpPopover label={`What can a ${m.role} do?`}>
                      {ROLE_HELP[m.role]}
                    </HelpPopover>
                  </td>
                  <td>
                    <Badge tone="warn" dot>
                      {STATUS_LABEL.invited}
                    </Badge>
                  </td>
                  <td>{formatJoined(m.joined_at)}</td>
                  <td>
                    {/* Resend/cancel-invite endpoints don't exist yet (BE-6). Shown
                        clearly disabled rather than faked. */}
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled
                      title="Resend is coming soon — not yet available."
                    >
                      Resend
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="lin-t3 lin-mt">
            These teammates have been emailed an invitation and will move to Members once they
            accept. Resending and canceling invitations aren&apos;t available yet.
          </p>
        </Card>
      ) : null}

      <ConfirmDialog
        open={removeTarget != null}
        title="Remove this member?"
        danger
        confirmLabel="Remove member"
        body={
          removeTarget ? (
            <Callout tone="danger">
              Remove <strong>{removeTarget.email}</strong>? This also revokes their API tokens
              and ends their access to this tenant immediately. This can&apos;t be undone.
            </Callout>
          ) : null
        }
        onClose={() => setRemoveTarget(null)}
        onConfirm={onConfirmRemove}
      />
    </div>
  );
}

/** Public entry — wraps the screen in a local ToastProvider so `useToast` works
 *  even when this client is mounted bare (the customer layout does not provide
 *  one; nested providers are harmless — the innermost wins). */
export function TeamClient(): React.ReactElement {
  return (
    <ToastProvider>
      <TeamInner />
    </ToastProvider>
  );
}

export default TeamClient;
