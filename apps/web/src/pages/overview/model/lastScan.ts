import type { AssessmentStatus } from "@/entities/scan-session";

/**
 * The last scan finished but Lumen could not decide about what it found: it
 * has no summary, yet saying "No scan yet" would be wrong.
 */
export function assessmentFailed(session: {
  readonly scanning: boolean;
  readonly scan: unknown;
  readonly assessment: AssessmentStatus;
}): boolean {
  return !session.scanning && session.scan !== null && session.assessment === "error";
}
