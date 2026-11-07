use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TranslateProviderConfig {
    #[serde(rename = "deeplx")]
    DeepLX { endpoint: String },
    #[serde(rename = "deepl")]
    DeepL { api_key: String, use_free_api: bool },
    #[serde(rename = "generic_api")]
    GenericApi {
        endpoint: String,
        api_key: Option<String>,
        source_lang_param: String,
        target_lang_param: String,
        text_param: String,
        result_path: String,
    },
    #[serde(rename = "llm")]
    LLM {
        endpoint: String,
        api_key: String,
        model: String,
        mode: LLMMode,
    },
    /// Microsoft Translator. Without an `api_key` this uses Microsoft's public
    /// keyless endpoint (the backend behind Bing Translator), so translation
    /// works with no setup at all. With a key it uses Azure Translator.
    #[serde(rename = "microsoft")]
    Microsoft {
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default)]
        region: Option<String>,
        #[serde(default)]
        endpoint: Option<String>,
    },
    /// Baidu Translate (百度翻译开放平台). `app_id` + `secret_key` from the
    /// developer console; every request is signed with an MD5 digest.
    #[serde(rename = "baidu")]
    Baidu {
        app_id: String,
        secret_key: String,
        #[serde(default)]
        endpoint: Option<String>,
    },
    /// Tencent Cloud Machine Translation (TMT). Signed with TC3-HMAC-SHA256.
    #[serde(rename = "tencent")]
    Tencent {
        secret_id: String,
        secret_key: String,
        #[serde(default)]
        region: Option<String>,
        #[serde(default)]
        project_id: Option<i64>,
        #[serde(default)]
        endpoint: Option<String>,
    },
    /// Alibaba Cloud Machine Translation (通用版). RPC signature, HMAC-SHA1.
    #[serde(rename = "aliyun")]
    Aliyun {
        access_key_id: String,
        access_key_secret: String,
        #[serde(default)]
        region: Option<String>,
        #[serde(default)]
        endpoint: Option<String>,
    },
    /// Volcengine Machine Translation (火山翻译). V4 signature, HMAC-SHA256.
    #[serde(rename = "volcengine")]
    Volcengine {
        access_key: String,
        secret_key: String,
        #[serde(default)]
        region: Option<String>,
        #[serde(default)]
        endpoint: Option<String>,
    },
    /// Huawei Cloud NLP text translation (华为云文本翻译). Needs the project ID
    /// from the console as well as the AK/SK pair; signed with SDK-HMAC-SHA256.
    #[serde(rename = "huawei")]
    Huawei {
        access_key: String,
        secret_key: String,
        project_id: String,
        #[serde(default)]
        region: Option<String>,
        #[serde(default)]
        endpoint: Option<String>,
    },
    /// Caiyun Xiaoyi (彩云小译). A single bearer-style token, no signature.
    #[serde(rename = "caiyun")]
    Caiyun {
        token: String,
        #[serde(default)]
        endpoint: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LLMMode {
    Completions,
    Responses,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslateResult {
    pub translated: String,
    pub segments: Vec<BilingualSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BilingualSegment {
    pub source: String,
    pub target: String,
}
