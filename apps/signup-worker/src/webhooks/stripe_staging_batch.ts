/** Request-scoped D1 transaction assembly for admitted Stripe webhooks. */

import type { D1PreparedStatement, D1RunResult } from "./billing_checkout";
import {
    ownershipInsertStatement,
    type StagingOwnershipContext,
} from "../staging_load_test_ownership.js";

export interface StripeStagingWrite {
    statements: D1PreparedStatement[];
    validate?: (results: D1RunResult[]) => void;
    atomicGuards?: D1PreparedStatement[];
}

/** Collects the exact prepared statements used by the ordinary writer path. */
export class StripeStagingWriteBatch {
    readonly writes: StripeStagingWrite[] = [];

    add(
        statement: D1PreparedStatement,
        validate?: (result: D1RunResult) => void,
        atomicGuards: D1PreparedStatement[] = [],
    ): void {
        this.writes.push({
            statements: [statement],
            validate: validate ? (results) => validate(results[0] ?? {}) : undefined,
            atomicGuards,
        });
    }

    addGroup(
        statements: D1PreparedStatement[],
        validate?: (results: D1RunResult[]) => void,
        atomicGuards: D1PreparedStatement[] = [],
    ): void {
        this.writes.push({ statements, validate, atomicGuards });
    }
}

export async function runOrStage(
    statement: D1PreparedStatement,
    batch?: StripeStagingWriteBatch,
    validate?: (result: D1RunResult) => void,
    atomicGuards: D1PreparedStatement[] = [],
): Promise<D1RunResult> {
    if (!batch) return statement.run();
    batch.add(statement, validate, atomicGuards);
    // Preserve existing local validation branches. The actual result is
    // checked by `validate` after the enclosing D1 batch commits.
    return { meta: { changes: 1 } };
}

export function changesGuard(db: D1DatabaseLikeBatch, predicate: string): D1PreparedStatement {
    return db.prepare(`INSERT INTO staging_load_test_resources
        (run_id, scenario, resource_class, receipt_ref, opaque_handle, disposition, state, registered_at_ms)
      SELECT '', 'webhook', 'webhook_effect', '', '', 'disposable', 'invalid', 0
      WHERE ${predicate}`);
}

const REPLAY_GUARD = `INSERT INTO staging_load_test_resources
    (run_id, scenario, resource_class, receipt_ref, opaque_handle, disposition, state, registered_at_ms)
  SELECT '', 'webhook', 'webhook_inbox', '', '', 'disposable', 'invalid', 0
  WHERE changes() = 0`;

/** Commit claim, all collected mutations, and their ownership rows atomically. */
export async function commitStripeStagingWrites(
    db: D1DatabaseLikeBatch,
    context: StagingOwnershipContext,
    event: { id: string; type: string },
    outcome: string,
    correlationId: string,
    nowMs: number,
    batch: StripeStagingWriteBatch,
): Promise<D1RunResult> {
    if (!db.batch) throw new Error("staging webhook requires atomic D1 batches");
    const claim = db.prepare(
        `INSERT OR IGNORE INTO stripe_webhook_events_processed
           (event_id, event_type, processed_at_ms, outcome, correlation_id)
         VALUES (?1, ?2, ?3, ?4, ?5)`,
    ).bind(event.id, event.type, nowMs,
        outcome, correlationId);
    const statements: D1PreparedStatement[] = [claim, db.prepare(REPLAY_GUARD)];
    const verify: Array<{ start: number; length: number; check: StripeStagingWrite["validate"] }> = [];
    let ordinal = 0;
    const appendOwnership = async (resourceClass: "webhook_inbox" | "webhook_effect", handle: string) => {
        statements.push(await ownershipInsertStatement(
            db as unknown as D1Database,
            context,
            resourceClass,
            "disposable",
            handle,
            nowMs,
        ));
        statements.push(db.prepare(REPLAY_GUARD));
    };
    // A signed admission request is single-use even when a sender changes the
    // Stripe event body under the same envelope.
    await appendOwnership("webhook_inbox", await opaqueHandle(context, "", "admission"));
    for (const write of batch.writes) {
        const start = statements.length;
        statements.push(...write.statements);
        statements.push(...(write.atomicGuards ?? []));
        for (let index = 0; index < write.statements.length; index++) {
            await appendOwnership("webhook_effect", await opaqueHandle(context, event.id, `${ordinal++}`));
        }
        verify.push({ start, length: write.statements.length, check: write.validate });
    }
    const results = await db.batch(statements);
    if ((results[0]?.meta?.changes ?? 0) !== 1) throw new Error("staging webhook replay rejected");
    for (const write of verify) {
        write.check?.(results.slice(write.start, write.start + write.length));
    }
    return results[0] ?? {};
}

async function opaqueHandle(context: StagingOwnershipContext, eventId: string, operation: string): Promise<string> {
    const digest = await crypto.subtle.digest(
        "SHA-256",
        new TextEncoder().encode(`corelink/webhook-effect-handle/v1\0${context.runId}\0${context.targetDeploymentSha}\0${context.requestId}\0${eventId}\0${operation}`),
    );
    return `stripe-webhook:${Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

type D1DatabaseLikeBatch = {
    prepare(query: string): D1PreparedStatement;
    batch?(statements: D1PreparedStatement[]): Promise<D1RunResult[]>;
};
