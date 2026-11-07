import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const REPO = "zi-ha/Pebble";

function read(relativePath: string) {
  return readFileSync(resolve(process.cwd(), relativePath), "utf8");
}

describe("release repository", () => {
  it("checks for updates against the same repository the About page links to", () => {
    const health = read("src-tauri/src/commands/health.rs");
    const about = read("src/features/settings/AboutTab.tsx");

    expect(health).toContain(`const REPO: &str = "${REPO}"`);
    expect(about).toContain(`const REPO = "${REPO}"`);
    expect(health).toContain("api.github.com/repos/{REPO}/releases/latest");
  });

  it("never points readers at a repository this project does not own", () => {
    for (const file of ["README.md", "README.zh-CN.md", "CHANGELOG.md"]) {
      expect(read(file)).not.toContain("QingJ01");
    }
  });

  it("links badges and clone instructions at the release repository", () => {
    const readme = read("README.md");

    expect(readme).toContain(`img.shields.io/github/v/release/${REPO}`);
    expect(readme).toContain(`git clone https://github.com/${REPO}.git`);
  });
});
