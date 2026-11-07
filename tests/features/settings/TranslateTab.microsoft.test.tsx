import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import TranslateTab from "../../../src/features/settings/TranslateTab";
import { getTranslateConfig, saveTranslateConfig, testTranslateConnection } from "../../../src/lib/api";

vi.mock("../../../src/lib/api", () => ({
  getTranslateConfig: vi.fn(),
  saveTranslateConfig: vi.fn(),
  testTranslateConnection: vi.fn(),
}));

vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: vi.fn() },
  useTranslation: () => ({
    t: (key: string, fallback?: string | Record<string, unknown>) => {
      if (fallback !== null && typeof fallback === "object") return key;
      const labels: Record<string, string> = {
        "translate.providerMicrosoft": "Microsoft Translator (built in)",
        "translate.providerDeeplx": "DeepLX",
        "translate.providerDeepl": "DeepL",
        "translate.providerGeneric": "Generic API",
        "translate.providerLlm": "LLM",
        "translate.enableTranslation": "Enable translation",
        "translate.provider": "Provider",
        "translate.apiKeyOptional": "API Key (optional)",
        "translate.azureRegion": "Azure region",
        "translate.microsoftKeyPlaceholder": "Leave empty to use the built-in free endpoint",
        "translate.microsoftBuiltinHint": "Built in and free — no setup needed.",
        "translate.testConnection": "Test Connection",
        "translate.configSaved": "Configuration saved.",
        "common.save": "Save",
        "common.saving": "Saving...",
        "common.testing": "Testing...",
      };
      return labels[key] ?? fallback ?? key;
    },
  }),
}));

const getConfigMock = vi.mocked(getTranslateConfig);
const saveConfigMock = vi.mocked(saveTranslateConfig);
const testConnectionMock = vi.mocked(testTranslateConnection);

describe("TranslateTab Microsoft engine", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getConfigMock.mockResolvedValue(null);
    saveConfigMock.mockResolvedValue(undefined);
    testConnectionMock.mockResolvedValue("你好");
  });

  it("defaults to the built-in Microsoft engine with no key required", async () => {
    render(<TranslateTab />);

    const provider = (await screen.findByLabelText("Provider")) as HTMLSelectElement;
    expect(provider.value).toBe("microsoft");

    // The hint explains the built-in endpoint, and the key stays optional.
    expect(screen.getByText(/Built in and free/i)).toBeTruthy();
    expect(screen.getByLabelText("API Key (optional)")).toBeTruthy();
    // No region field until an Azure key is typed.
    expect(screen.queryByLabelText("Azure region")).toBeNull();
  });

  it("saves a keyless Microsoft config so the built-in endpoint is used", async () => {
    render(<TranslateTab />);

    fireEvent.click(await screen.findByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveConfigMock).toHaveBeenCalledTimes(1));
    const [providerType, configJson, enabled] = saveConfigMock.mock.calls[0];
    expect(providerType).toBe("microsoft");
    expect(enabled).toBe(true);
    expect(JSON.parse(configJson)).toEqual({
      type: "microsoft",
      api_key: null,
      region: null,
      endpoint: null,
    });
  });

  it("forwards an Azure key and reveals the region field when one is entered", async () => {
    render(<TranslateTab />);

    fireEvent.change(await screen.findByLabelText("API Key (optional)"), {
      target: { value: "azure-key" },
    });

    const region = await screen.findByLabelText("Azure region");
    fireEvent.change(region, { target: { value: "eastasia" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveConfigMock).toHaveBeenCalledTimes(1));
    expect(JSON.parse(saveConfigMock.mock.calls[0][1])).toEqual({
      type: "microsoft",
      api_key: "azure-key",
      region: "eastasia",
      endpoint: null,
    });
  });

  it("tests the keyless config against the built-in endpoint", async () => {
    render(<TranslateTab />);

    fireEvent.click(await screen.findByRole("button", { name: "Test Connection" }));

    await waitFor(() => expect(testConnectionMock).toHaveBeenCalledTimes(1));
    expect(JSON.parse(testConnectionMock.mock.calls[0][0])).toMatchObject({
      type: "microsoft",
      api_key: null,
    });
  });

  it("keeps an existing engine when one is already saved", async () => {
    getConfigMock.mockResolvedValue({
      id: "active",
      provider_type: "deeplx",
      config: JSON.stringify({ type: "deeplx", endpoint: "http://localhost:1188/translate" }),
      is_enabled: false,
      created_at: 0,
      updated_at: 0,
    });

    render(<TranslateTab />);

    const provider = (await screen.findByLabelText("Provider")) as HTMLSelectElement;
    await waitFor(() => expect(provider.value).toBe("deeplx"));
    expect((screen.getByLabelText("Enable translation") as HTMLInputElement).checked).toBe(false);
  });
});
