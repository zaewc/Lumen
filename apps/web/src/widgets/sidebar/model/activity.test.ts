import { describe, expect, it } from "vitest";
import { headerActivity, type Running } from "./activity";

const idle: Running = {
  installing: false,
  moving: false,
  command: false,
  scanning: false,
  deciding: false,
};

describe("headerActivity", () => {
  it("names nothing while nothing runs", () => {
    expect(headerActivity(idle, "/quarantine")).toBeNull();
  });

  it("leaves a scan to the pages that show it", () => {
    const scanning = { ...idle, scanning: true };
    expect(headerActivity(scanning, "/clean")).toBeNull();
    expect(headerActivity(scanning, "/")).toBeNull();
    expect(headerActivity(scanning, "/settings")).toBe("scanning");
    expect(headerActivity({ ...idle, deciding: true }, "/quarantine")).toBe("deciding");
  });

  it("leaves an update to Settings and a move to Storage and Explore", () => {
    expect(headerActivity({ ...idle, installing: true }, "/settings")).toBeNull();
    expect(headerActivity({ ...idle, installing: true }, "/clean")).toBe("installing");
    expect(headerActivity({ ...idle, moving: true }, "/clean")).toBeNull();
    expect(headerActivity({ ...idle, moving: true }, "/quarantine")).toBe("moving");
  });

  it("always names a running command, and the first hidden work gives way to the next", () => {
    expect(headerActivity({ ...idle, command: true }, "/clean")).toBe("command");
    expect(headerActivity({ ...idle, installing: true, scanning: true }, "/settings")).toBe(
      "scanning",
    );
  });

  it("names everything where the place is unknown", () => {
    expect(headerActivity({ ...idle, scanning: true }, null)).toBe("scanning");
  });
});
