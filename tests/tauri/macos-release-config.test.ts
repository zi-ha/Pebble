import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("macOS release configuration", () => {
  it("keeps release version metadata in sync", () => {
    const packageJson = JSON.parse(readFileSync(resolve(process.cwd(), "package.json"), "utf8"));
    const tauriConfig = JSON.parse(
      readFileSync(resolve(process.cwd(), "src-tauri", "tauri.conf.json"), "utf8"),
    );
    const cargoToml = readFileSync(resolve(process.cwd(), "src-tauri", "Cargo.toml"), "utf8");
    const changelog = readFileSync(resolve(process.cwd(), "CHANGELOG.md"), "utf8");
    const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

    expect(packageJson.version).toMatch(/^\d+\.\d+\.\d+$/);
    expect(tauriConfig.version).toBe(packageJson.version);
    expect(cargoVersion).toBe(packageJson.version);
    expect(changelog).toContain(`## [${packageJson.version}] - `);
    expect(changelog).toContain(`[Unreleased]: https://github.com/zi-ha/Pebble/compare/v${packageJson.version}...HEAD`);
  });

  it("defines explicit desktop build scripts for Windows, macOS, and Linux bundles", () => {
    const packageJson = JSON.parse(readFileSync(resolve(process.cwd(), "package.json"), "utf8"));

    expect(packageJson.scripts["build:windows"]).toBeTypeOf("string");
    expect(packageJson.scripts["build:macos"]).toBeTypeOf("string");
    expect(packageJson.scripts["build:linux"]).toBeTypeOf("string");
    expect(packageJson.scripts["build:windows"]).toContain("--bundles nsis");
    expect(packageJson.scripts["build:macos"]).toContain("--bundles app,dmg");
    expect(packageJson.scripts["build:linux"]).toContain("--bundles appimage,deb,rpm");
  });

  it("builds through the explicit per-platform scripts, not a wrapper", () => {
    const packageJson = JSON.parse(readFileSync(resolve(process.cwd(), "package.json"), "utf8"));

    // `scripts/build-tauri.mjs` was removed: every build entry point is now an
    // explicit `tauri build --bundles ...`, so the produced installer is obvious
    // from the command itself instead of hidden behind a platform map.
    expect(packageJson.scripts.build).toBeUndefined();
    expect(existsSync(resolve(process.cwd(), "scripts"))).toBe(false);
  });

  it("keeps desktop notification clicks routable to the target message", () => {
    const notificationSource = readFileSync(
      resolve(process.cwd(), "src-tauri", "src", "commands", "notification_center.rs"),
      "utf8",
    ).replace(/\r\n/g, "\n");
    const eventsSource = readFileSync(resolve(process.cwd(), "src-tauri", "src", "events.rs"), "utf8").replace(
      /\r\n/g,
      "\n",
    );
    const cargoToml = readFileSync(resolve(process.cwd(), "src-tauri", "Cargo.toml"), "utf8");

    expect(eventsSource).toContain('pub const MAIL_NOTIFICATION_OPEN: &str = "mail:notification-open";');
    expect(eventsSource).not.toContain('#[cfg(windows)]\npub const MAIL_NOTIFICATION_OPEN');
    expect(notificationSource).toContain("fn open_payload");
    expect(notificationSource).not.toContain("#[cfg(any(windows, test))]\nfn open_payload");
    expect(notificationSource).toContain("fn open_message");
    expect(notificationSource).not.toContain("#[cfg(windows)]\nfn open_message");
    expect(notificationSource).toContain("fn show_linux_notification");
    expect(notificationSource).toContain("wait_for_action");
    expect(notificationSource).toContain("fn show_macos_notification");
    expect(notificationSource).toContain("wait_for_click(true)");
    expect(cargoToml).toContain('[target.\'cfg(target_os = "linux")\'.dependencies]');
    expect(cargoToml).toContain('notify-rust = "4"');
    expect(cargoToml).toContain('[target.\'cfg(target_os = "macos")\'.dependencies]');
    expect(cargoToml).toContain('mac-notification-sys = "0.6"');
  });

  it("includes a macOS icon in the Tauri bundle config", () => {
    const config = JSON.parse(
      readFileSync(resolve(process.cwd(), "src-tauri", "tauri.conf.json"), "utf8"),
    );

    expect(config.bundle.icon).toContain("icons/icon.icns");
    expect(existsSync(resolve(process.cwd(), "src-tauri", "icons", "icon.icns"))).toBe(true);
  });

  it("enables native credential storage backends for Windows, macOS, and Linux", () => {
    const cargoToml = readFileSync(resolve(process.cwd(), "Cargo.toml"), "utf8");

    expect(cargoToml).toContain('"apple-native"');
    expect(cargoToml).toContain('"windows-native"');
    expect(cargoToml).toContain('"linux-native-sync-persistent"');
    expect(cargoToml).toContain('"crypto-rust"');
  });
});
