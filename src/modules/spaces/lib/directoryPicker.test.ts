import { describe, expect, it } from "vitest";
import { joinDirectory, parentDirectory } from "./directoryPicker";

describe("directory picker paths", () => {
  it("joins child directories using canonical forward slashes", () => {
    expect(joinDirectory("C:/Users/me", "repo")).toBe("C:/Users/me/repo");
    expect(joinDirectory("/home/me/", "repo")).toBe("/home/me/repo");
  });

  it("preserves Unix and Windows filesystem roots when navigating up", () => {
    expect(parentDirectory("/home/me")).toBe("/home");
    expect(parentDirectory("C:/Users/me")).toBe("C:/Users");
    expect(parentDirectory("C:/")).toBe("C:/");
  });
});
