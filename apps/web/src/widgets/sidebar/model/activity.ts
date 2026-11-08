/** Work running in this session that the header can name. */
export type Activity = "installing" | "moving" | "command" | "scanning" | "deciding";

/** What is running, as the session and the hosts report it. */
export interface Running {
  readonly installing: boolean;
  readonly moving: boolean;
  readonly command: boolean;
  readonly scanning: boolean;
  readonly deciding: boolean;
}

/** The places of the app, as the navigation lists them. */
export type Place = "/" | "/clean" | "/quarantine" | "/apps" | "/settings";

/** The places that already show each kind of work themselves. */
const SHOWN_ON: { readonly [A in Activity]: readonly Place[] } = {
  installing: ["/settings"],
  moving: ["/", "/clean"],
  command: [],
  scanning: ["/", "/clean"],
  deciding: ["/", "/clean"],
};

/** The most important work first: what holds quitting, then what can wait. */
const ORDER: readonly Activity[] = ["installing", "moving", "command", "scanning", "deciding"];

/**
 * The work the header names on `place`: the first running one that the
 * page does not show already, so one message is never on screen twice.
 */
export function headerActivity(running: Running, place: Place | null): Activity | null {
  return (
    ORDER.find(
      (activity) => running[activity] && (place === null || !SHOWN_ON[activity].includes(place)),
    ) ?? null
  );
}
