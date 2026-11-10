//! User-adjustable execution limits. Defaults and validation live with the wire contract.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeField {
    pub key: String,
    pub label: String,
    pub unit: String,
    pub help: String,
    pub min: f64,
    pub max: f64,
    pub group: String,
    pub advanced: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeSettings {
    pub anydoc_timeout_secs: u64,
    pub docling_timeout_secs: u64,
    pub conversion_input_mib: usize,
    pub conversion_output_mib: usize,
    pub docling_threads: usize,
    pub pdf_parser_threads: usize,
    pub pdf_concurrent_results: usize,
    pub ocr_batch_size: usize,
    pub layout_batch_size: usize,
    pub table_batch_size: usize,
    pub queue_max_size: usize,
    pub max_document_pages: usize,
    pub conversion_memory_percent: u64,
    pub conversion_job_mib: u64,
    pub text_preparation_mib: usize,
    pub max_text_input_mib: usize,
    pub max_decoded_text_mib: usize,
    pub max_prepared_text_mib: usize,
    pub query_workers: usize,
    pub query_cache_entries: usize,
    pub corpus_cache_entries: usize,
    pub jev_concurrency: usize,
    pub jev_context_tokens: usize,
    pub retry_count: usize,
    pub retry_base_secs: u64,
    pub retry_max_secs: u64,
    pub retry_total_secs: u64,
    pub fold_concurrency: usize,
    pub fold_part_units: usize,
    pub watch_debounce_ms: u64,
    pub watch_max_delay_ms: u64,
    pub do_ocr: bool,
    pub do_table_structure: bool,
}
impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            anydoc_timeout_secs: 120,
            docling_timeout_secs: 1800,
            conversion_input_mib: 512,
            conversion_output_mib: 16,
            docling_threads: 2,
            pdf_parser_threads: 1,
            pdf_concurrent_results: 2,
            ocr_batch_size: 1,
            layout_batch_size: 1,
            table_batch_size: 1,
            queue_max_size: 2,
            max_document_pages: 10000,
            conversion_memory_percent: 50,
            conversion_job_mib: 0,
            text_preparation_mib: 512,
            max_text_input_mib: 16,
            max_decoded_text_mib: 32,
            max_prepared_text_mib: 64,
            query_workers: 8,
            query_cache_entries: 256,
            corpus_cache_entries: 8,
            jev_concurrency: 20,
            jev_context_tokens: 32000,
            retry_count: 3,
            retry_base_secs: 1,
            retry_max_secs: 30,
            retry_total_secs: 600,
            fold_concurrency: 4,
            fold_part_units: 40,
            watch_debounce_ms: 300,
            watch_max_delay_ms: 1500,
            do_ocr: true,
            do_table_structure: true,
        }
    }
}
impl RuntimeSettings {
    /// Only settings affecting parsed content invalidate successful conversion caches.
    pub fn content_identity(&self) -> String {
        format!(
            "parse-v2:ocr={}:table={}:pages={}",
            self.do_ocr, self.do_table_structure, self.max_document_pages
        )
    }
    pub fn docling_outer_timeout_secs(&self) -> u64 {
        self.docling_timeout_secs.saturating_add(60)
    }
    pub fn validate(&self) -> Result<(), String> {
        let values = serde_json::to_value(self).map_err(|error| error.to_string())?;
        for field in fields() {
            if let Some(value) = values[&field.key].as_f64() {
                if !(field.min..=field.max).contains(&value) {
                    return Err(format!("{}应为 {}–{}", field.label, field.min, field.max));
                }
            }
        }
        if self.conversion_output_mib > self.max_text_input_mib {
            return Err("转换正文上限不能大于索引正文输入上限".into());
        }
        if self.max_decoded_text_mib < self.max_text_input_mib
            || self.max_prepared_text_mib < self.max_decoded_text_mib
        {
            return Err("正文大小限制应满足：输入 ≤ 解码 ≤ 准备".into());
        }
        if self.max_prepared_text_mib > self.text_preparation_mib {
            return Err("准备后正文上限不能大于索引文本准备预算".into());
        }
        if self.retry_max_secs < self.retry_base_secs {
            return Err("最大重试间隔不能小于初始间隔".into());
        }
        if self.watch_max_delay_ms < self.watch_debounce_ms {
            return Err("最长合并窗口不能小于合并等待".into());
        }
        Ok(())
    }
}
pub fn fields() -> Vec<RuntimeField> {
    vec![
        RuntimeField {
            key: "anydoc_timeout_secs".into(),
            label: "内置转换超时".into(),
            unit: "秒".into(),
            help: "新转换任务生效".into(),
            min: 1.0,
            max: 86400.0,
            group: "files".into(),
            advanced: false,
        },
        RuntimeField {
            key: "docling_timeout_secs".into(),
            label: "Docling 转换超时".into(),
            unit: "秒".into(),
            help: "外层自动额外预留 60 秒用于退出清理".into(),
            min: 1.0,
            max: 86400.0,
            group: "files".into(),
            advanced: false,
        },
        RuntimeField {
            key: "conversion_input_mib".into(),
            label: "转换输入大小上限".into(),
            unit: "MiB".into(),
            help: "仅限制原始文档".into(),
            min: 1.0,
            max: 4096.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "conversion_output_mib".into(),
            label: "转换正文大小上限".into(),
            unit: "MiB".into(),
            help: "必须不大于索引正文输入上限".into(),
            min: 1.0,
            max: 1024.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "docling_threads".into(),
            label: "Docling 推理线程数".into(),
            unit: "".into(),
            help: "新转换任务生效".into(),
            min: 1.0,
            max: 64.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "pdf_parser_threads".into(),
            label: "PDF 解析线程数".into(),
            unit: "".into(),
            help: "新转换任务生效".into(),
            min: 1.0,
            max: 64.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "pdf_concurrent_results".into(),
            label: "PDF 并行结果数".into(),
            unit: "".into(),
            help: "新转换任务生效".into(),
            min: 1.0,
            max: 64.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "ocr_batch_size".into(),
            label: "OCR 批量大小".into(),
            unit: "".into(),
            help: "较大会增加内存占用".into(),
            min: 1.0,
            max: 64.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "layout_batch_size".into(),
            label: "版面识别批量大小".into(),
            unit: "".into(),
            help: "较大会增加内存占用".into(),
            min: 1.0,
            max: 64.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "table_batch_size".into(),
            label: "表格识别批量大小".into(),
            unit: "".into(),
            help: "较大会增加内存占用".into(),
            min: 1.0,
            max: 64.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "queue_max_size".into(),
            label: "Docling 队列容量".into(),
            unit: "".into(),
            help: "新转换任务生效".into(),
            min: 1.0,
            max: 128.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "max_document_pages".into(),
            label: "本地文档最大页数".into(),
            unit: "页".into(),
            help: "不改变云端服务的页数限制".into(),
            min: 1.0,
            max: 100000.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "conversion_memory_percent".into(),
            label: "转换总内存预算".into(),
            unit: "%".into(),
            help: "物理内存百分比；配置并发仍受此预算限制".into(),
            min: 10.0,
            max: 80.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "conversion_job_mib".into(),
            label: "单任务内存预算".into(),
            unit: "MiB".into(),
            help: "0 表示自动计算，实际不超过总预算".into(),
            min: 0.0,
            max: 262144.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "text_preparation_mib".into(),
            label: "索引文本准备预算".into(),
            unit: "MiB".into(),
            help: "索引任务启动后固定".into(),
            min: 64.0,
            max: 8192.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "max_text_input_mib".into(),
            label: "索引正文输入上限".into(),
            unit: "MiB".into(),
            help: "同时适用于转换后的 Markdown".into(),
            min: 1.0,
            max: 1024.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "max_decoded_text_mib".into(),
            label: "解码后正文上限".into(),
            unit: "MiB".into(),
            help: "不能小于正文输入上限".into(),
            min: 1.0,
            max: 2048.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "max_prepared_text_mib".into(),
            label: "准备后正文上限".into(),
            unit: "MiB".into(),
            help: "不能小于解码后正文上限".into(),
            min: 1.0,
            max: 4096.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "query_workers".into(),
            label: "本地查询工作线程数".into(),
            unit: "".into(),
            help: "新查询生效".into(),
            min: 1.0,
            max: 64.0,
            group: "retrieval".into(),
            advanced: true,
        },
        RuntimeField {
            key: "query_cache_entries".into(),
            label: "查询缓存条目数".into(),
            unit: "".into(),
            help: "超限时淘汰最旧条目".into(),
            min: 1.0,
            max: 4096.0,
            group: "retrieval".into(),
            advanced: true,
        },
        RuntimeField {
            key: "corpus_cache_entries".into(),
            label: "语料缓存条目数".into(),
            unit: "".into(),
            help: "超限时淘汰最旧条目".into(),
            min: 1.0,
            max: 128.0,
            group: "retrieval".into(),
            advanced: true,
        },
        RuntimeField {
            key: "jev_concurrency".into(),
            label: "JEV 请求并发数".into(),
            unit: "".into(),
            help: "每次检索的最大并发请求数".into(),
            min: 1.0,
            max: 64.0,
            group: "retrieval".into(),
            advanced: true,
        },
        RuntimeField {
            key: "jev_context_tokens".into(),
            label: "JEV 单批上下文预算".into(),
            unit: "tokens".into(),
            help: "应与所用 JEV 服务能力匹配".into(),
            min: 4096.0,
            max: 262144.0,
            group: "retrieval".into(),
            advanced: true,
        },
        RuntimeField {
            key: "retry_count".into(),
            label: "模型失败重试次数".into(),
            unit: "".into(),
            help: "包括 HTTP 499；首次请求不计入重试".into(),
            min: 0.0,
            max: 10.0,
            group: "models".into(),
            advanced: true,
        },
        RuntimeField {
            key: "retry_base_secs".into(),
            label: "模型重试初始间隔".into(),
            unit: "秒".into(),
            help: "按指数退避".into(),
            min: 1.0,
            max: 60.0,
            group: "models".into(),
            advanced: true,
        },
        RuntimeField {
            key: "retry_max_secs".into(),
            label: "模型重试最大间隔".into(),
            unit: "秒".into(),
            help: "不能小于初始间隔".into(),
            min: 1.0,
            max: 300.0,
            group: "models".into(),
            advanced: true,
        },
        RuntimeField {
            key: "retry_total_secs".into(),
            label: "模型请求总时限".into(),
            unit: "秒".into(),
            help: "覆盖重试、等待与响应读取".into(),
            min: 1.0,
            max: 86400.0,
            group: "models".into(),
            advanced: true,
        },
        RuntimeField {
            key: "fold_concurrency".into(),
            label: "文档折叠并发数".into(),
            unit: "".into(),
            help: "新折叠任务生效".into(),
            min: 1.0,
            max: 32.0,
            group: "answer".into(),
            advanced: true,
        },
        RuntimeField {
            key: "fold_part_units".into(),
            label: "文档折叠每批段落数".into(),
            unit: "".into(),
            help: "超出模型预算时仍会自动拆小".into(),
            min: 1.0,
            max: 200.0,
            group: "answer".into(),
            advanced: true,
        },
        RuntimeField {
            key: "watch_debounce_ms".into(),
            label: "文件变更合并等待".into(),
            unit: "ms".into(),
            help: "新事件批次生效".into(),
            min: 50.0,
            max: 10000.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "watch_max_delay_ms".into(),
            label: "文件变更最长合并窗口".into(),
            unit: "ms".into(),
            help: "不能小于合并等待".into(),
            min: 50.0,
            max: 60000.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "do_ocr".into(),
            label: "启用本地 OCR".into(),
            unit: "".into(),
            help: "新转换任务生效".into(),
            min: 0.0,
            max: 1.0,
            group: "files".into(),
            advanced: true,
        },
        RuntimeField {
            key: "do_table_structure".into(),
            label: "启用本地表格结构识别".into(),
            unit: "".into(),
            help: "新转换任务生效".into(),
            min: 0.0,
            max: 1.0,
            group: "files".into(),
            advanced: true,
        },
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_covers_every_runtime_field_and_only_content_changes_invalidate_cache() {
        let runtime = RuntimeSettings::default();
        let values = serde_json::to_value(&runtime).unwrap();
        let schema = fields();
        assert_eq!(schema.len(), values.as_object().unwrap().len());
        for field in schema {
            assert!(!values[&field.key].is_null(), "{}", field.key);
        }
        let mut scheduling = runtime.clone();
        scheduling.docling_timeout_secs += 1;
        scheduling.jev_concurrency += 1;
        assert_eq!(runtime.content_identity(), scheduling.content_identity());
        scheduling.do_ocr = false;
        assert_ne!(runtime.content_identity(), scheduling.content_identity());
    }
    #[test]
    fn old_preferences_receive_runtime_defaults() {
        let settings: RuntimeSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.docling_timeout_secs, 1800);
        assert_eq!(settings.docling_outer_timeout_secs(), 1860);
        assert_eq!(settings.jev_concurrency, 20);
        assert!(settings.validate().is_ok());
    }
    #[test]
    fn rejects_zero_workers_inconsistent_limits_and_unbounded_retries() {
        let mut settings = RuntimeSettings::default();
        settings.jev_concurrency = 0;
        assert!(settings.validate().is_err());
        settings = RuntimeSettings::default();
        settings.max_decoded_text_mib = settings.max_text_input_mib - 1;
        assert!(settings.validate().is_err());
        settings = RuntimeSettings::default();
        settings.retry_total_secs = 0;
        assert!(settings.validate().is_err());
    }
}
