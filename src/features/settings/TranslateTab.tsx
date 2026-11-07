import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import {
  getTranslateConfig,
  saveTranslateConfig,
  testTranslateConnection,
} from "../../lib/api";
import { useToastStore } from "@/stores/toast.store";
import { extractErrorMessage } from "../../lib/extractErrorMessage";

type ProviderType =
  | "microsoft"
  | "deeplx"
  | "deepl"
  | "generic_api"
  | "llm"
  | "baidu"
  | "tencent"
  | "aliyun"
  | "volcengine"
  | "huawei"
  | "caiyun";

const PROVIDER_OPTIONS: { value: ProviderType; labelKey: string; fallback: string }[] = [
  { value: "microsoft", labelKey: "translate.providerMicrosoft", fallback: "Microsoft (built in)" },
  { value: "deeplx", labelKey: "translate.providerDeeplx", fallback: "DeepLX" },
  { value: "deepl", labelKey: "translate.providerDeepl", fallback: "DeepL" },
  { value: "baidu", labelKey: "translate.providerBaidu", fallback: "Baidu Translate" },
  { value: "tencent", labelKey: "translate.providerTencent", fallback: "Tencent Cloud TMT" },
  { value: "aliyun", labelKey: "translate.providerAliyun", fallback: "Alibaba Cloud MT" },
  { value: "volcengine", labelKey: "translate.providerVolcengine", fallback: "Volcengine Translate" },
  { value: "huawei", labelKey: "translate.providerHuawei", fallback: "Huawei Cloud NLP" },
  { value: "caiyun", labelKey: "translate.providerCaiyun", fallback: "Caiyun Xiaoyi" },
  { value: "generic_api", labelKey: "translate.providerGeneric", fallback: "Generic API" },
  { value: "llm", labelKey: "translate.providerLlm", fallback: "LLM" },
];

const labelStyle: React.CSSProperties = {
  display: "block",
  fontSize: "12px",
  fontWeight: 500,
  color: "var(--color-text-secondary)",
  marginBottom: "4px",
};

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 10px",
  fontSize: "13px",
  border: "1px solid var(--color-border)",
  borderRadius: "6px",
  background: "var(--color-bg-secondary)",
  color: "var(--color-text-primary)",
  boxSizing: "border-box",
};

const fieldGroupStyle: React.CSSProperties = {
  marginBottom: "14px",
};

const buttonStyle: React.CSSProperties = {
  padding: "8px 18px",
  fontSize: "13px",
  fontWeight: 500,
  border: "none",
  borderRadius: "6px",
  cursor: "pointer",
};

const hintStyle: React.CSSProperties = {
  ...fieldGroupStyle,
  padding: "10px 12px",
  borderRadius: "6px",
  background: "var(--color-bg-hover)",
  fontSize: "12.5px",
  lineHeight: 1.55,
  color: "var(--color-text-secondary)",
};

/** One credential input — every cloud vendor needs at least two of these. */
function CredentialField({
  id,
  name,
  label,
  value,
  onChange,
  secret = false,
  placeholder,
}: {
  id: string;
  name: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  secret?: boolean;
  placeholder?: string;
}) {
  return (
    <div style={fieldGroupStyle}>
      <label htmlFor={id} style={labelStyle}>
        {label}
      </label>
      <input
        id={id}
        name={name}
        style={inputStyle}
        type={secret ? "password" : "text"}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        autoComplete={secret ? "current-password" : "off"}
      />
    </div>
  );
}

export default function TranslateTab() {
  const { t } = useTranslation();
  const [providerType, setProviderType] = useState<ProviderType>("microsoft");
  // The built-in engine is available out of the box, so a fresh install starts
  // enabled; a saved config still overrides this on load.
  const [isEnabled, setIsEnabled] = useState(true);

  // Microsoft Translator fields. Both are optional: without a key the built-in
  // (keyless) Microsoft endpoint is used, with a key the Azure endpoint is used.
  const [microsoftApiKey, setMicrosoftApiKey] = useState("");
  const [microsoftRegion, setMicrosoftRegion] = useState("");

  // DeepLX fields
  const [deeplxEndpoint, setDeeplxEndpoint] = useState("");

  // DeepL fields
  const [deeplApiKey, setDeeplApiKey] = useState("");
  const [deeplUseFree, setDeeplUseFree] = useState(false);

  // Generic API fields
  const [genericEndpoint, setGenericEndpoint] = useState("");
  const [genericApiKey, setGenericApiKey] = useState("");
  const [genericSourceLangParam, setGenericSourceLangParam] = useState("source_lang");
  const [genericTargetLangParam, setGenericTargetLangParam] = useState("target_lang");
  const [genericTextParam, setGenericTextParam] = useState("text");
  const [genericResultPath, setGenericResultPath] = useState("data");

  // LLM fields
  const [llmEndpoint, setLlmEndpoint] = useState("");
  const [llmApiKey, setLlmApiKey] = useState("");
  const [llmModel, setLlmModel] = useState("");
  const [llmMode, setLlmMode] = useState<"completions" | "responses">("completions");

  // Baidu Translate fields
  const [baiduAppId, setBaiduAppId] = useState("");
  const [baiduSecretKey, setBaiduSecretKey] = useState("");

  // Tencent Cloud TMT fields
  const [tencentSecretId, setTencentSecretId] = useState("");
  const [tencentSecretKey, setTencentSecretKey] = useState("");
  const [tencentRegion, setTencentRegion] = useState("ap-guangzhou");

  // Alibaba Cloud Machine Translation fields
  const [aliyunAccessKeyId, setAliyunAccessKeyId] = useState("");
  const [aliyunAccessKeySecret, setAliyunAccessKeySecret] = useState("");
  const [aliyunRegion, setAliyunRegion] = useState("cn-hangzhou");

  // Volcengine Machine Translation fields
  const [volcAccessKey, setVolcAccessKey] = useState("");
  const [volcSecretKey, setVolcSecretKey] = useState("");
  const [volcRegion, setVolcRegion] = useState("cn-north-1");

  // Huawei Cloud NLP text translation fields
  const [huaweiAccessKey, setHuaweiAccessKey] = useState("");
  const [huaweiSecretKey, setHuaweiSecretKey] = useState("");
  const [huaweiProjectId, setHuaweiProjectId] = useState("");
  const [huaweiRegion, setHuaweiRegion] = useState("cn-north-4");

  // Caiyun Xiaoyi fields
  const [caiyunToken, setCaiyunToken] = useState("");

  const [statusMsg, setStatusMsg] = useState("");
  const [statusType, setStatusType] = useState<"success" | "error" | "">("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    loadConfig();
  }, []);

  async function loadConfig() {
    try {
      const cfg = await getTranslateConfig();
      if (!cfg) return;

      setProviderType(cfg.provider_type as ProviderType);
      setIsEnabled(cfg.is_enabled);

      const parsed = JSON.parse(cfg.config);
      switch (parsed.type) {
        case "microsoft":
          setMicrosoftApiKey(parsed.api_key || "");
          setMicrosoftRegion(parsed.region || "");
          break;
        case "deeplx":
          setDeeplxEndpoint(parsed.endpoint || "");
          break;
        case "deepl":
          setDeeplApiKey(parsed.api_key || "");
          setDeeplUseFree(parsed.use_free_api ?? false);
          break;
        case "generic_api":
          setGenericEndpoint(parsed.endpoint || "");
          setGenericApiKey(parsed.api_key || "");
          setGenericSourceLangParam(parsed.source_lang_param || "source_lang");
          setGenericTargetLangParam(parsed.target_lang_param || "target_lang");
          setGenericTextParam(parsed.text_param || "text");
          setGenericResultPath(parsed.result_path || "data");
          break;
        case "llm":
          setLlmEndpoint(parsed.endpoint || "");
          setLlmApiKey(parsed.api_key || "");
          setLlmModel(parsed.model || "");
          setLlmMode(parsed.mode || "completions");
          break;
        case "baidu":
          setBaiduAppId(parsed.app_id || "");
          setBaiduSecretKey(parsed.secret_key || "");
          break;
        case "tencent":
          setTencentSecretId(parsed.secret_id || "");
          setTencentSecretKey(parsed.secret_key || "");
          setTencentRegion(parsed.region || "ap-guangzhou");
          break;
        case "aliyun":
          setAliyunAccessKeyId(parsed.access_key_id || "");
          setAliyunAccessKeySecret(parsed.access_key_secret || "");
          setAliyunRegion(parsed.region || "cn-hangzhou");
          break;
        case "volcengine":
          setVolcAccessKey(parsed.access_key || "");
          setVolcSecretKey(parsed.secret_key || "");
          setVolcRegion(parsed.region || "cn-north-1");
          break;
        case "huawei":
          setHuaweiAccessKey(parsed.access_key || "");
          setHuaweiSecretKey(parsed.secret_key || "");
          setHuaweiProjectId(parsed.project_id || "");
          setHuaweiRegion(parsed.region || "cn-north-4");
          break;
        case "caiyun":
          setCaiyunToken(parsed.token || "");
          break;
      }
    } catch (err) {
      console.error("Failed to load translate config:", err);
    }
  }

  function buildConfigJson(): string {
    switch (providerType) {
      case "microsoft":
        return JSON.stringify({
          type: "microsoft",
          api_key: microsoftApiKey.trim() || null,
          region: microsoftRegion.trim() || null,
          endpoint: null,
        });
      case "deeplx":
        return JSON.stringify({ type: "deeplx", endpoint: deeplxEndpoint });
      case "deepl":
        return JSON.stringify({ type: "deepl", api_key: deeplApiKey, use_free_api: deeplUseFree });
      case "generic_api":
        return JSON.stringify({
          type: "generic_api",
          endpoint: genericEndpoint,
          api_key: genericApiKey || null,
          method: null,
          source_lang_param: genericSourceLangParam,
          target_lang_param: genericTargetLangParam,
          text_param: genericTextParam,
          result_path: genericResultPath,
        });
      case "llm":
        return JSON.stringify({
          type: "llm",
          endpoint: llmEndpoint,
          api_key: llmApiKey,
          model: llmModel,
          mode: llmMode,
        });
      case "baidu":
        return JSON.stringify({
          type: "baidu",
          app_id: baiduAppId.trim(),
          secret_key: baiduSecretKey.trim(),
          endpoint: null,
        });
      case "tencent":
        return JSON.stringify({
          type: "tencent",
          secret_id: tencentSecretId.trim(),
          secret_key: tencentSecretKey.trim(),
          region: tencentRegion.trim() || null,
          project_id: 0,
          endpoint: null,
        });
      case "aliyun":
        return JSON.stringify({
          type: "aliyun",
          access_key_id: aliyunAccessKeyId.trim(),
          access_key_secret: aliyunAccessKeySecret.trim(),
          region: aliyunRegion.trim() || null,
          endpoint: null,
        });
      case "volcengine":
        return JSON.stringify({
          type: "volcengine",
          access_key: volcAccessKey.trim(),
          secret_key: volcSecretKey.trim(),
          region: volcRegion.trim() || null,
          endpoint: null,
        });
      case "huawei":
        return JSON.stringify({
          type: "huawei",
          access_key: huaweiAccessKey.trim(),
          secret_key: huaweiSecretKey.trim(),
          project_id: huaweiProjectId.trim(),
          region: huaweiRegion.trim() || null,
          endpoint: null,
        });
      case "caiyun":
        return JSON.stringify({
          type: "caiyun",
          token: caiyunToken.trim(),
          endpoint: null,
        });
    }
  }

  async function handleSave() {
    setSaving(true);
    setStatusMsg("");
    try {
      const configJson = buildConfigJson();
      await saveTranslateConfig(providerType, configJson, isEnabled);
      setStatusMsg(t("translate.configSaved"));
      setStatusType("success");
    } catch (err: unknown) {
      const errMsg = extractErrorMessage(err);
      setStatusMsg(t("translate.saveFailed", { error: errMsg }));
      setStatusType("error");
      useToastStore.getState().addToast({
        message: t("translate.saveFailed", { error: errMsg }),
        type: "error",
      });
    } finally {
      setSaving(false);
    }
  }

  async function handleTest() {
    setTesting(true);
    setStatusMsg("");
    try {
      const configJson = buildConfigJson();
      const result = await testTranslateConnection(configJson);
      setStatusMsg(t("translate.connectionOk", { result }));
      setStatusType("success");
    } catch (err: unknown) {
      const errMsg = extractErrorMessage(err);
      setStatusMsg(t("translate.testFailed", { error: errMsg }));
      setStatusType("error");
      useToastStore.getState().addToast({
        message: t("translate.testFailed", { error: errMsg }),
        type: "error",
      });
    } finally {
      setTesting(false);
    }
  }

  function renderProviderFields() {
    switch (providerType) {
      case "microsoft":
        return (
          <>
            <div
              style={{
                ...fieldGroupStyle,
                padding: "10px 12px",
                borderRadius: "6px",
                background: "var(--color-bg-hover)",
                fontSize: "12.5px",
                lineHeight: 1.55,
                color: "var(--color-text-secondary)",
              }}
            >
              {t(
                "translate.microsoftBuiltinHint",
                "Built in and free: no account or API key needed. Leave the key empty to use Microsoft's public endpoint. Fill in a key only if you want to use your own Azure Translator subscription.",
              )}
            </div>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-microsoft-api-key" style={labelStyle}>
                {t("translate.apiKeyOptional")}
              </label>
              <input
                id="translate-microsoft-api-key"
                name="microsoft_api_key"
                style={inputStyle}
                type="password"
                value={microsoftApiKey}
                onChange={(e) => setMicrosoftApiKey(e.target.value)}
                placeholder={t("translate.microsoftKeyPlaceholder", "Leave empty to use the built-in free endpoint")}
                autoComplete="current-password"
              />
            </div>
            {microsoftApiKey.trim() !== "" && (
              <div style={fieldGroupStyle}>
                <label htmlFor="translate-microsoft-region" style={labelStyle}>
                  {t("translate.azureRegion", "Azure region (required for Azure keys)")}
                </label>
                <input
                  id="translate-microsoft-region"
                  name="microsoft_region"
                  style={inputStyle}
                  value={microsoftRegion}
                  onChange={(e) => setMicrosoftRegion(e.target.value)}
                  placeholder="eastasia"
                  autoComplete="off"
                />
              </div>
            )}
          </>
        );

      case "deeplx":
        return (
          <div style={fieldGroupStyle}>
            <label htmlFor="translate-deeplx-endpoint" style={labelStyle}>{t("translate.endpointUrl")}</label>
            <input
              id="translate-deeplx-endpoint"
              name="deeplx_endpoint"
              type="url"
              style={inputStyle}
              value={deeplxEndpoint}
              onChange={(e) => setDeeplxEndpoint(e.target.value)}
              placeholder="http://localhost:1188/translate"
              autoComplete="off"
            />
          </div>
        );

      case "deepl":
        return (
          <>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-deepl-api-key" style={labelStyle}>{t("translate.apiKey")}</label>
              <input
                id="translate-deepl-api-key"
                name="deepl_api_key"
                style={inputStyle}
                type="password"
                value={deeplApiKey}
                onChange={(e) => setDeeplApiKey(e.target.value)}
                placeholder="your-deepl-api-key"
                autoComplete="current-password"
              />
            </div>
            <div style={{ ...fieldGroupStyle, display: "flex", alignItems: "center", gap: "8px" }}>
              <input
                type="checkbox"
                checked={deeplUseFree}
                onChange={(e) => setDeeplUseFree(e.target.checked)}
                id="deepl-free"
              />
              <label htmlFor="deepl-free" style={{ fontSize: "13px", color: "var(--color-text-primary)", cursor: "pointer" }}>
                {t("translate.useFreeApi")}
              </label>
            </div>
          </>
        );

      case "baidu":
        return (
          <>
            <div style={hintStyle}>
              {t(
                "translate.cloudCredentialHint",
                "Create a key pair in the vendor's machine-translation console, then paste it here. Credentials are stored encrypted on this device.",
              )}
            </div>
            <CredentialField
              id="translate-baidu-app-id"
              name="baidu_app_id"
              label={t("translate.baiduAppId", "AppID")}
              value={baiduAppId}
              onChange={setBaiduAppId}
              placeholder="20240101000000001"
            />
            <CredentialField
              id="translate-baidu-secret-key"
              name="baidu_secret_key"
              label={t("translate.baiduSecretKey", "Secret Key")}
              value={baiduSecretKey}
              onChange={setBaiduSecretKey}
              secret
            />
          </>
        );

      case "tencent":
        return (
          <>
            <div style={hintStyle}>
              {t(
                "translate.cloudCredentialHint",
                "Create a key pair in the vendor's machine-translation console, then paste it here. Credentials are stored encrypted on this device.",
              )}
            </div>
            <CredentialField
              id="translate-tencent-secret-id"
              name="tencent_secret_id"
              label={t("translate.tencentSecretId", "SecretId")}
              value={tencentSecretId}
              onChange={setTencentSecretId}
              placeholder="AKID..."
            />
            <CredentialField
              id="translate-tencent-secret-key"
              name="tencent_secret_key"
              label={t("translate.tencentSecretKey", "SecretKey")}
              value={tencentSecretKey}
              onChange={setTencentSecretKey}
              secret
            />
            <CredentialField
              id="translate-tencent-region"
              name="tencent_region"
              label={t("translate.tencentRegion", "Region")}
              value={tencentRegion}
              onChange={setTencentRegion}
              placeholder="ap-guangzhou"
            />
          </>
        );

      case "aliyun":
        return (
          <>
            <div style={hintStyle}>
              {t(
                "translate.cloudCredentialHint",
                "Create a key pair in the vendor's machine-translation console, then paste it here. Credentials are stored encrypted on this device.",
              )}
            </div>
            <CredentialField
              id="translate-aliyun-access-key-id"
              name="aliyun_access_key_id"
              label={t("translate.aliyunAccessKeyId", "AccessKey ID")}
              value={aliyunAccessKeyId}
              onChange={setAliyunAccessKeyId}
              placeholder="LTAI..."
            />
            <CredentialField
              id="translate-aliyun-access-key-secret"
              name="aliyun_access_key_secret"
              label={t("translate.aliyunAccessKeySecret", "AccessKey Secret")}
              value={aliyunAccessKeySecret}
              onChange={setAliyunAccessKeySecret}
              secret
            />
            <CredentialField
              id="translate-aliyun-region"
              name="aliyun_region"
              label={t("translate.aliyunRegion", "RegionId")}
              value={aliyunRegion}
              onChange={setAliyunRegion}
              placeholder="cn-hangzhou"
            />
          </>
        );

      case "volcengine":
        return (
          <>
            <div style={hintStyle}>
              {t(
                "translate.cloudCredentialHint",
                "Create a key pair in the vendor's machine-translation console, then paste it here. Credentials are stored encrypted on this device.",
              )}
            </div>
            <CredentialField
              id="translate-volc-access-key"
              name="volc_access_key"
              label={t("translate.volcAccessKey", "Access Key (AK)")}
              value={volcAccessKey}
              onChange={setVolcAccessKey}
              placeholder="AKLT..."
            />
            <CredentialField
              id="translate-volc-secret-key"
              name="volc_secret_key"
              label={t("translate.volcSecretKey", "Secret Key (SK)")}
              value={volcSecretKey}
              onChange={setVolcSecretKey}
              secret
            />
            <CredentialField
              id="translate-volc-region"
              name="volc_region"
              label={t("translate.volcRegion", "Region")}
              value={volcRegion}
              onChange={setVolcRegion}
              placeholder="cn-north-1"
            />
          </>
        );

      case "huawei":
        return (
          <>
            <div style={hintStyle}>
              {t(
                "translate.huaweiHint",
                "开通「自然语言处理 NLP · 文本翻译」后，在「我的凭证」里拿到 AK/SK 和项目 ID（Project ID）填在下面。AK/SK 用于签名，项目 ID 是请求路径的一部分。",
              )}
            </div>
            <CredentialField
              id="translate-huawei-access-key"
              name="huawei_access_key"
              label={t("translate.huaweiAccessKey", "Access Key (AK)")}
              value={huaweiAccessKey}
              onChange={setHuaweiAccessKey}
              placeholder="QTWA..."
            />
            <CredentialField
              id="translate-huawei-secret-key"
              name="huawei_secret_key"
              label={t("translate.huaweiSecretKey", "Secret Key (SK)")}
              value={huaweiSecretKey}
              onChange={setHuaweiSecretKey}
              secret
            />
            <CredentialField
              id="translate-huawei-project-id"
              name="huawei_project_id"
              label={t("translate.huaweiProjectId", "Project ID")}
              value={huaweiProjectId}
              onChange={setHuaweiProjectId}
              placeholder="77b6a44cba5143ab91d13ab9a8ff44fd"
            />
            <CredentialField
              id="translate-huawei-region"
              name="huawei_region"
              label={t("translate.huaweiRegion", "Region")}
              value={huaweiRegion}
              onChange={setHuaweiRegion}
              placeholder="cn-north-4"
            />
          </>
        );

      case "caiyun":
        return (
          <>
            <div style={hintStyle}>
              {t(
                "translate.caiyunHint",
                "在彩云小译开放平台申请访问令牌（Token）填在下面即可。该接口只支持中文、英文、日文之间互译。",
              )}
            </div>
            <CredentialField
              id="translate-caiyun-token"
              name="caiyun_token"
              label={t("translate.caiyunToken", "Access Token")}
              value={caiyunToken}
              onChange={setCaiyunToken}
              secret
            />
          </>
        );

      case "generic_api":
        return (
          <>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-generic-endpoint" style={labelStyle}>{t("translate.endpoint")}</label>
              <input
                id="translate-generic-endpoint"
                name="generic_endpoint"
                type="url"
                style={inputStyle}
                value={genericEndpoint}
                onChange={(e) => setGenericEndpoint(e.target.value)}
                placeholder="https://api.example.com/translate"
                autoComplete="off"
              />
            </div>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-generic-api-key" style={labelStyle}>{t("translate.apiKeyOptional")}</label>
              <input
                id="translate-generic-api-key"
                name="generic_api_key"
                style={inputStyle}
                type="password"
                value={genericApiKey}
                onChange={(e) => setGenericApiKey(e.target.value)}
                placeholder={t("translate.apiKeyOptional")}
                autoComplete="current-password"
              />
            </div>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-generic-source-param" style={labelStyle}>{t("translate.sourceLangParam")}</label>
              <input
                id="translate-generic-source-param"
                name="generic_source_lang_param"
                style={inputStyle}
                value={genericSourceLangParam}
                onChange={(e) => setGenericSourceLangParam(e.target.value)}
                autoComplete="off"
              />
            </div>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-generic-target-param" style={labelStyle}>{t("translate.targetLangParam")}</label>
              <input
                id="translate-generic-target-param"
                name="generic_target_lang_param"
                style={inputStyle}
                value={genericTargetLangParam}
                onChange={(e) => setGenericTargetLangParam(e.target.value)}
                autoComplete="off"
              />
            </div>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-generic-text-param" style={labelStyle}>{t("translate.textParam")}</label>
              <input
                id="translate-generic-text-param"
                name="generic_text_param"
                style={inputStyle}
                value={genericTextParam}
                onChange={(e) => setGenericTextParam(e.target.value)}
                autoComplete="off"
              />
            </div>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-generic-result-path" style={labelStyle}>{t("translate.resultPath")}</label>
              <input
                id="translate-generic-result-path"
                name="generic_result_path"
                style={inputStyle}
                value={genericResultPath}
                onChange={(e) => setGenericResultPath(e.target.value)}
                autoComplete="off"
              />
            </div>
          </>
        );

      case "llm":
        return (
          <>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-llm-endpoint" style={labelStyle}>{t("translate.endpoint")}</label>
              <input
                id="translate-llm-endpoint"
                name="llm_endpoint"
                type="url"
                style={inputStyle}
                value={llmEndpoint}
                onChange={(e) => setLlmEndpoint(e.target.value)}
                placeholder="https://api.openai.com/v1"
                autoComplete="off"
              />
            </div>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-llm-api-key" style={labelStyle}>{t("translate.apiKey")}</label>
              <input
                id="translate-llm-api-key"
                name="llm_api_key"
                style={inputStyle}
                type="password"
                value={llmApiKey}
                onChange={(e) => setLlmApiKey(e.target.value)}
                placeholder="your-api-key"
                autoComplete="current-password"
              />
            </div>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-llm-model" style={labelStyle}>{t("translate.model")}</label>
              <input
                id="translate-llm-model"
                name="llm_model"
                style={inputStyle}
                value={llmModel}
                onChange={(e) => setLlmModel(e.target.value)}
                placeholder="gpt-4o-mini"
                autoComplete="off"
              />
            </div>
            <div style={fieldGroupStyle}>
              <label htmlFor="translate-llm-mode" style={labelStyle}>{t("translate.mode")}</label>
              <select
                id="translate-llm-mode"
                name="llm_mode"
                style={inputStyle}
                value={llmMode}
                onChange={(e) => setLlmMode(e.target.value as "completions" | "responses")}
              >
                <option value="completions">{t("translate.modeCompletions")}</option>
                <option value="responses">{t("translate.modeResponses")}</option>
              </select>
            </div>
          </>
        );
    }
  }

  return (
    <div>
      <h2 style={{ fontSize: "18px", fontWeight: 600, color: "var(--color-text-primary)", marginTop: 0, marginBottom: "20px" }}>
        {t("translate.engineTitle")}
      </h2>

      {/* Enable toggle */}
      <div style={{ ...fieldGroupStyle, display: "flex", alignItems: "center", gap: "8px" }}>
        <input
          type="checkbox"
          checked={isEnabled}
          onChange={(e) => setIsEnabled(e.target.checked)}
          id="translate-enabled"
        />
        <label htmlFor="translate-enabled" style={{ fontSize: "13px", color: "var(--color-text-primary)", cursor: "pointer" }}>
          {t("translate.enableTranslation")}
        </label>
      </div>

      {/* Provider selector */}
      <div style={fieldGroupStyle}>
        <label htmlFor="translate-provider" style={labelStyle}>{t("translate.provider")}</label>
        <select
          id="translate-provider"
          name="translate_provider"
          style={inputStyle}
          value={providerType}
          onChange={(e) => setProviderType(e.target.value as ProviderType)}
        >
          {PROVIDER_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {t(opt.labelKey, opt.fallback)}
            </option>
          ))}
        </select>
      </div>

      {/* Dynamic provider fields */}
      {renderProviderFields()}

      {/* Actions */}
      <div style={{ display: "flex", gap: "10px", marginTop: "20px" }}>
        <button
          style={{
            ...buttonStyle,
            background: "var(--color-accent)",
            color: "#fff",
            opacity: saving ? 0.6 : 1,
          }}
          onClick={handleSave}
          disabled={saving}
        >
          {saving ? t("common.saving") : t("common.save")}
        </button>
        <button
          style={{
            ...buttonStyle,
            background: "var(--color-bg-hover)",
            color: "var(--color-text-primary)",
            opacity: testing ? 0.6 : 1,
          }}
          onClick={handleTest}
          disabled={testing}
        >
          {testing ? t("common.testing") : t("translate.testConnection")}
        </button>
      </div>

      {/* Status message */}
      {statusMsg && (
        <div
          role={statusType === "error" ? "alert" : "status"}
          aria-live="polite"
          style={{
            marginTop: "14px",
            padding: "10px 14px",
            borderRadius: "6px",
            fontSize: "13px",
            background: statusType === "success" ? "var(--color-bg-hover)" : "rgba(220, 53, 69, 0.1)",
            color: statusType === "success" ? "var(--color-text-primary)" : "#dc3545",
            border: `1px solid ${statusType === "success" ? "var(--color-border)" : "rgba(220, 53, 69, 0.3)"}`,
          }}
        >
          {statusMsg}
        </div>
      )}
    </div>
  );
}
