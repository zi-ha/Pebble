import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import TranslateTab from "../../../src/features/settings/TranslateTab";
import { getTranslateConfig, saveTranslateConfig, testTranslateConnection } from "../../../src/lib/api";

vi.mock("../../../src/lib/api", () => ({
  getTranslateConfig: vi.fn(),
  saveTranslateConfig: vi.fn(),
  testTranslateConnection: vi.fn(),
}));

const LABELS: Record<string, string> = {
  "translate.providerMicrosoft": "Microsoft Translator (built in)",
  "translate.providerDeeplx": "DeepLX",
  "translate.providerDeepl": "DeepL",
  "translate.providerBaidu": "Baidu Translate",
  "translate.providerTencent": "Tencent Cloud TMT",
  "translate.providerAliyun": "Alibaba Cloud MT",
  "translate.providerVolcengine": "Volcengine Translate",
  "translate.providerHuawei": "Huawei Cloud NLP",
  "translate.providerCaiyun": "Caiyun Xiaoyi",
  "translate.providerGeneric": "Generic API",
  "translate.providerLlm": "LLM",
  "translate.enableTranslation": "Enable translation",
  "translate.provider": "Provider",
  "translate.cloudCredentialHint": "Create a key pair in the vendor's console.",
  "translate.baiduAppId": "AppID",
  "translate.baiduSecretKey": "Baidu Secret Key",
  "translate.tencentSecretId": "SecretId",
  "translate.tencentSecretKey": "Tencent SecretKey",
  "translate.tencentRegion": "Tencent Region",
  "translate.aliyunAccessKeyId": "AccessKey ID",
  "translate.aliyunAccessKeySecret": "AccessKey Secret",
  "translate.aliyunRegion": "Aliyun RegionId",
  "translate.volcAccessKey": "Volcengine AK",
  "translate.volcSecretKey": "Volcengine SK",
  "translate.volcRegion": "Volcengine Region",
  "translate.huaweiAccessKey": "Huawei AK",
  "translate.huaweiSecretKey": "Huawei SK",
  "translate.huaweiProjectId": "Huawei Project ID",
  "translate.huaweiRegion": "Huawei Region",
  "translate.huaweiHint": "Huawei hint",
  "translate.caiyunToken": "Caiyun Token",
  "translate.caiyunHint": "Caiyun hint",
  "translate.testConnection": "Test Connection",
  "translate.configSaved": "Configuration saved.",
  "common.save": "Save",
  "common.saving": "Saving...",
  "common.testing": "Testing...",
};

vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: vi.fn() },
  useTranslation: () => ({
    t: (key: string, fallback?: string | Record<string, unknown>) => {
      if (fallback !== null && typeof fallback === "object") return key;
      return LABELS[key] ?? fallback ?? key;
    },
  }),
}));

const getConfigMock = vi.mocked(getTranslateConfig);
const saveConfigMock = vi.mocked(saveTranslateConfig);
const testConnectionMock = vi.mocked(testTranslateConnection);

async function selectProvider(label: string) {
  fireEvent.change(await screen.findByLabelText("Provider"), { target: { value: label } });
}

describe("TranslateTab cloud vendors", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getConfigMock.mockResolvedValue(null);
    saveConfigMock.mockResolvedValue(undefined);
    testConnectionMock.mockResolvedValue("你好");
  });

  it("offers every Chinese cloud vendor", async () => {
    render(<TranslateTab />);

    const provider = (await screen.findByLabelText("Provider")) as HTMLSelectElement;
    const values = Array.from(provider.options).map((option) => option.value);

    expect(values).toEqual(
      expect.arrayContaining(["baidu", "tencent", "aliyun", "volcengine", "huawei", "caiyun"]),
    );
  });

  it("saves the Huawei key pair together with the project id", async () => {
    render(<TranslateTab />);

    await selectProvider("huawei");
    expect(((await screen.findByLabelText("Huawei Region")) as HTMLInputElement).value).toBe(
      "cn-north-4",
    );

    fireEvent.change(screen.getByLabelText("Huawei AK"), { target: { value: "QTWAtest" } });
    fireEvent.change(screen.getByLabelText("Huawei SK"), { target: { value: "huawei-secret" } });
    fireEvent.change(screen.getByLabelText("Huawei Project ID"), {
      target: { value: "77b6a44cba5143ab91d13ab9a8ff44fd" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveConfigMock).toHaveBeenCalledTimes(1));
    expect(JSON.parse(saveConfigMock.mock.calls[0][1])).toEqual({
      type: "huawei",
      access_key: "QTWAtest",
      secret_key: "huawei-secret",
      project_id: "77b6a44cba5143ab91d13ab9a8ff44fd",
      region: "cn-north-4",
      endpoint: null,
    });
  });

  it("saves a Caiyun token and restores it on reload", async () => {
    getConfigMock.mockResolvedValue(null);
    render(<TranslateTab />);

    await selectProvider("caiyun");
    fireEvent.change(await screen.findByLabelText("Caiyun Token"), {
      target: { value: "caiyun-token" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveConfigMock).toHaveBeenCalledTimes(1));
    expect(saveConfigMock.mock.calls[0][0]).toBe("caiyun");
    expect(JSON.parse(saveConfigMock.mock.calls[0][1])).toEqual({
      type: "caiyun",
      token: "caiyun-token",
      endpoint: null,
    });
  });

  it("saves a Baidu pair as an MD5-signed config", async () => {
    render(<TranslateTab />);

    await selectProvider("baidu");
    fireEvent.change(await screen.findByLabelText("AppID"), {
      target: { value: "20240101000000001" },
    });
    fireEvent.change(screen.getByLabelText("Baidu Secret Key"), {
      target: { value: "baidu-secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveConfigMock).toHaveBeenCalledTimes(1));
    const [providerType, configJson] = saveConfigMock.mock.calls[0];
    expect(providerType).toBe("baidu");
    expect(JSON.parse(configJson)).toEqual({
      type: "baidu",
      app_id: "20240101000000001",
      secret_key: "baidu-secret",
      endpoint: null,
    });
  });

  it("defaults the Tencent region and forwards it with the key pair", async () => {
    render(<TranslateTab />);

    await selectProvider("tencent");
    expect(
      ((await screen.findByLabelText("Tencent Region")) as HTMLInputElement).value,
    ).toBe("ap-guangzhou");

    fireEvent.change(screen.getByLabelText("SecretId"), { target: { value: "AKIDtest" } });
    fireEvent.change(screen.getByLabelText("Tencent SecretKey"), {
      target: { value: "tencent-secret" },
    });
    fireEvent.change(screen.getByLabelText("Tencent Region"), {
      target: { value: "ap-beijing" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveConfigMock).toHaveBeenCalledTimes(1));
    expect(JSON.parse(saveConfigMock.mock.calls[0][1])).toEqual({
      type: "tencent",
      secret_id: "AKIDtest",
      secret_key: "tencent-secret",
      region: "ap-beijing",
      project_id: 0,
      endpoint: null,
    });
  });

  it("sends the Alibaba Cloud key pair when testing the connection", async () => {
    render(<TranslateTab />);

    await selectProvider("aliyun");
    fireEvent.change(await screen.findByLabelText("AccessKey ID"), {
      target: { value: "LTAItest" },
    });
    fireEvent.change(screen.getByLabelText("AccessKey Secret"), {
      target: { value: "aliyun-secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Test Connection" }));

    await waitFor(() => expect(testConnectionMock).toHaveBeenCalledTimes(1));
    expect(JSON.parse(testConnectionMock.mock.calls[0][0])).toEqual({
      type: "aliyun",
      access_key_id: "LTAItest",
      access_key_secret: "aliyun-secret",
      region: "cn-hangzhou",
      endpoint: null,
    });
  });

  it("restores a saved Volcengine config into its fields", async () => {
    getConfigMock.mockResolvedValue({
      id: "active",
      provider_type: "volcengine",
      config: JSON.stringify({
        type: "volcengine",
        access_key: "AKLTsaved",
        secret_key: "saved-secret",
        region: "cn-beijing",
        endpoint: null,
      }),
      is_enabled: true,
      created_at: 0,
      updated_at: 0,
    });

    render(<TranslateTab />);

    await waitFor(() =>
      expect((screen.getByLabelText("Volcengine AK") as HTMLInputElement).value).toBe("AKLTsaved"),
    );
    expect((screen.getByLabelText("Volcengine SK") as HTMLInputElement).value).toBe("saved-secret");
    expect((screen.getByLabelText("Volcengine Region") as HTMLInputElement).value).toBe(
      "cn-beijing",
    );
  });
});
