# ADR-034: Interaction Generations and Serialized Cancellation

- Status: Accepted for Phase 0E-R1
- Date: 2026-08-10
- Refines ADR-030

## Context

Clearing only the visible drag state does not cancel a scheduled intent or an update already in flight. Rolling back before that update settles lets an old response mutate the post-cancel state. Worker restart has the same stale-response risk.

## Decision

Every interaction queue has a monotonically increasing generation. A queued operation captures its generation and is ignored if it is stale. Escape and pointercancel increment the generation, remove scheduled/latest intent, await the in-flight request, and only then issue rollback. Worker restart runs the same cancellation boundary before terminating waiters and replacing the Worker.

Transform and geometry updates remain absolute final intents. Pan remains an accumulated delta so event coalescing does not lose movement. React publishes overlay state only after the matching Worker response and GPU frame sequence.

## Consequences

- Old-generation queued work cannot execute after rollback.
- DOM pointercancel leaves the transaction inactive, history unchanged, the semantic pre-drag state restored, and request/GPU/overlay sequences converged.
- Restart cannot make an earlier fallback or console/GPU error disappear from the run-wide maximum/error record.

