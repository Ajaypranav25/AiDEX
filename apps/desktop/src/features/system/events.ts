// Plain-language rendering of event envelopes. The catalog keeps growing (crates/events/src/catalog),
// so unknown types render generically from their type and data instead of being dropped.
import type { ActivityEntry } from "@aidex/ui/components";
import type { EventEnvelope } from "@aidex/client/dto/daemon";

const FAILED = /(failed|denied|cancelled|interrupted|expired|error)$/;
const SUCCEEDED = /(completed|succeeded|approved|finished|created|unlocked)$/;

function describe(envelope: EventEnvelope): string {
  const name = envelope.event.type.replace(/[._]/g, " ");
  const detail = Object.entries(envelope.event.data ?? {})
    .filter(([, value]) => value !== null && value !== undefined && typeof value !== "object")
    .map(([key, value]) => `${key.replace(/_/g, " ")}: ${String(value)}`)
    .join(", ");
  return detail ? `${name} (${detail})` : name;
}

function timeOf(envelope: EventEnvelope): string {
  const date = new Date(envelope.occurred_at);
  return Number.isNaN(date.getTime()) ? `#${envelope.cursor}` : date.toLocaleTimeString();
}

/** One `ActivityLog` entry per envelope. */
export function activityEntry(envelope: EventEnvelope): ActivityEntry {
  const type = envelope.event.type;
  return {
    id: String(envelope.cursor),
    time: timeOf(envelope),
    text: describe(envelope),
    tone: FAILED.test(type) ? "danger" : SUCCEEDED.test(type) ? "success" : "default",
  };
}
