import { describe, expect, it } from "vitest";
import { baseName } from "./path";

describe("baseName", () => {
  it("takes the last component of a Unix path", () => {
    expect(baseName("/Users/ana/Library/Caches/com.example.app")).toBe("com.example.app");
    expect(baseName("/Users/ana/node_modules/")).toBe("node_modules");
    expect(baseName("relative/dir")).toBe("dir");
  });

  it("takes the last component of a Windows path", () => {
    expect(baseName("C:\\Users\\ana\\AppData\\Local\\Temp")).toBe("Temp");
    expect(baseName("C:\\Users\\ana\\Downloads\\")).toBe("Downloads");
    expect(baseName("\\\\server\\share\\folder")).toBe("folder");
    expect(baseName("C:/Users/ana/mixed\\sep")).toBe("sep");
  });

  it("shows a root or an empty path whole", () => {
    expect(baseName("/")).toBe("/");
    expect(baseName("C:\\")).toBe("C:\\");
    expect(baseName("D:")).toBe("D:");
    expect(baseName("")).toBe("");
  });
});
