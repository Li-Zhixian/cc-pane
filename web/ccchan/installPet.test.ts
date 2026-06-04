import { describe, expect, it } from "vitest";
import { isCCPanesPetInstallLink } from "./installPet";

describe("isCCPanesPetInstallLink", () => {
  it("accepts CC-Panes pet install links only", () => {
    expect(isCCPanesPetInstallLink(
      "ccpanes://pets/install?name=Doro&imageUrl=https%3A%2F%2Fexample.invalid%2Fdoro.webp",
    )).toBe(true);
    expect(isCCPanesPetInstallLink(
      "ccpanes://pets/install/?name=猫&image_url=https%3A%2F%2Fexample.invalid%2Fcat.webp",
    )).toBe(true);
    expect(isCCPanesPetInstallLink("ccpanes://pets/other?name=Doro")).toBe(false);
    expect(isCCPanesPetInstallLink("codex://pets/install?name=Doro")).toBe(false);
    expect(isCCPanesPetInstallLink("https://example.invalid/pet.zip")).toBe(false);
    expect(isCCPanesPetInstallLink("not a url")).toBe(false);
  });
});
