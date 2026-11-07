import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const WORKFLOW_PATH = ".github/workflows/release-manual.yml";

function read(relativePath: string) {
  return readFileSync(resolve(process.cwd(), relativePath), "utf8");
}

describe("manual release workflow", () => {
  const workflow = read(WORKFLOW_PATH);

  it("is the only workflow in the repository", () => {
    // Everything else (CI, the multi-platform release) was dropped on purpose:
    // one hand-started workflow keeps the fork cheap and predictable.
    expect(readdirSync(resolve(process.cwd(), ".github", "workflows"))).toEqual(["release-manual.yml"]);
  });

  it("is started by hand and never by a tag push", () => {
    expect(workflow).toContain("workflow_dispatch:");
    expect(workflow).not.toMatch(/^\s+push:/m);
    expect(workflow).not.toMatch(/^\s+tags:/m);
  });

  it("takes the version from the app instead of a typed-in tag", () => {
    // The version must come from the manifest Tauri bakes into the binary, so the
    // released tag can never disagree with what the About page reports.
    expect(workflow).toMatch(/Get-Content src-tauri\/tauri\.conf\.json/);
    expect(workflow).toContain('"RELEASE_TAG=$tag"');
    expect(workflow).not.toMatch(/\bv\d+\.\d+\.\d+/);
    expect(workflow).not.toContain("inputs.tag");
  });

  it("refuses to release when the manifests disagree or the changelog has no notes", () => {
    expect(workflow).toContain("package.json");
    expect(workflow).toContain("src-tauri/Cargo.toml");
    expect(workflow).toContain("Version drift");
    expect(workflow).toContain('CHANGELOG.md has no');
  });

  it("refuses to release a version that already has a tag", () => {
    expect(workflow).toContain('git rev-parse -q --verify "refs/tags/$tag"');
    expect(workflow).toContain("already exists");
  });

  it("uploads the built installer to the release it created", () => {
    expect(workflow).toContain("pnpm build:windows");
    expect(workflow).toContain("gh release create");
    expect(workflow).toContain("gh release upload");
    expect(workflow).toContain("--target");
  });
});
