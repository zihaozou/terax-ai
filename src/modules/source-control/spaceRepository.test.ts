import { describe, expect, it } from "vitest";
import { sourceControlPathForSpace } from "./spaceRepository";

describe("sourceControlPathForSpace", () => {
  it("uses only the active Space root", () => {
    expect(sourceControlPathForSpace("/repo/packages/app", undefined)).toBe(
      "/repo/packages/app",
    );
  });

  it("pauses Source Control for an unavailable root", () => {
    expect(
      sourceControlPathForSpace("/missing", {
        candidate: "/missing",
        message: "not found",
      }),
    ).toBeNull();
  });
});
