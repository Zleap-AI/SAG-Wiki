export type MineruConfig={enabled:boolean;provider:'302'|'official';base_url?:string;has_key:boolean;request_timeout_secs:number;task_timeout_secs:number;poll_interval_secs:number;concurrency:number;model_version:'vlm'|'pipeline';version?:'2.0'|'2.5';parse_method?:'auto'|'txt'|'ocr';ocr:boolean;table:boolean;formula:boolean};
import type {ReasoningEffort} from './reasoning';
import type {Language} from './i18n';
export type Root={path:string;name:string;status:string;count:number;error:string|null};
export const chatSources={
 claude:{name:'Claude Code',folder:'claude code'},
 claude_desktop:{name:'Claude Desktop',folder:'claude desktop'},
 codex:{name:'Codex',folder:'codex'},
 workbuddy:{name:'WorkBuddy',folder:'workbuddy'},
} as const;
export type ChatSource=keyof typeof chatSources;
export type ArtifactSource='claude-code'|'codex'|'claude-desktop'|'workbuddy';
export type ChatSession={id:string;title:string;project:string;updated:string;paths:string[]};
export type ChatCatalog={source_root:string;sessions:ChatSession[];issues:{path:string;error:string}[];total:number;page:number;page_size:number;projects:string[]};
export type ChatPreview={title:string;body:string};
export type ChatImportItem={path:string;status:'imported'|'updated'|'skipped'|'failed';error:string|null};
export type ChatImportSchedule={enabled:boolean;time:string;interval_minutes?:number|null;source_root:string|null;include_tools:boolean};
export type ChatImportScheduleStatus={schedule:ChatImportSchedule;last_run_at:number|null;last_run:{changed:number;failed:number;error:string|null}|null};
export type Entry={path:string;name:string;directory:boolean;symlink:boolean};
export type Document={id:number;path:string;source_path?:string;name:string;body:string;hash:string};
export type FileInfo={id:number;path:string;source_path?:string;name:string;index_status?:'basic'|'enhancing'|'complete'|'enhancement_failed'|'awaiting_ocr'|'awaiting_conversion';index_error?:string|null};
export type SourceSpan={start_byte:number;end_byte:number;start_line:number;start_column:number;end_line:number;end_column:number};
export type AnswerContext={limit:number;reserved_output:number;estimated_input:number;omitted_sources:number;omitted_history:number;input_tokens:number|null;output_tokens:number|null;elapsed_ms?:number|null;generation_ms?:number|null;summary?:{text:string;covered_turns:number}|null;summary_error?:string|null};
export type Candidate={id:number;document_id:number;path:string;source_path?:string;title:string;heading:string;content:string;start_line:number;end_line:number;source_ranges:[number,number][];source_spans?:SourceSpan[];bm25:number;score:number;order:number;hash:string};
export type SearchSummaryResult={text:string;evidence:Candidate[];context:AnswerContext};
export type ModelConfig={reasoning_effort?:ReasoningEffort|null;auth_mode?:'bearer'|'none';url:string;model:string;models:string[];has_key:boolean};
export type McpServer={id:string;name:string;transport:'http'|'stdio';url:string;command:string;args:string[];require_confirmation:boolean;has_credentials:boolean;tools?:McpToolInfo[]|null};
export type McpCredentials={env:Record<string,string>;headers:Record<string,string>};
export type McpToolInfo={name:string;description:string};
export type ModelProfile={reasoning_effort?:ReasoningEffort|null;system_prompt:string;sag2_enabled?:boolean;mcp_servers:string[];mcp_tools?:Record<string,string[]>};
export type ToolApproval={id:string;query_id:string;server_name:string;tool_name:string;arguments:Record<string,unknown>};
export type Retrieval={candidate_k:number;seed_k:number;expansion_term_limit:number;max_hops:number;top_k:number;chunk_chars:number};
export type FileTypes={markdown:boolean;text:boolean;code:boolean;config:boolean};
export type UpdateSettings={source:'github'|'website';github_url:string;website_url:string;follow_language?:boolean};
export type UpdateInfo={current_version:string;latest_version:string;available:boolean;notes:string;download_url:string|null};
export type UpdateProgress={stage:'downloading'|'verifying';downloaded:number;total:number|null};
export type NoteSettings={folder?:string|null;ask_location?:boolean};
export type DocumentParser='docling'|'mineru'|'vision';
export type ConversionStorage={mode:"app_data"|"library"}|{mode:"custom";directory:string};
export type ProxyMode='system'|'custom'|'none';
export type ModelProxy={mode:ProxyMode;url:string;username:string;bypass:string;has_password:boolean;password?:string|null};
export const defaultModelProxy:ModelProxy={mode:'system',url:'',username:'',bypass:'',has_password:false,password:null};
/** Settings saved by 0.4.0 carry only url and bypass; a non-empty address meant a custom proxy. */
export function normalizeModelProxy(value?:Partial<ModelProxy>|null):ModelProxy{
 const url=value?.url??'';
 return {...defaultModelProxy,...value,mode:value?.mode??(url.trim()?'custom':'system'),url,username:value?.username??'',bypass:value?.bypass??'',has_password:value?.has_password??false,password:value?.password??null};
}
export type Settings={chat_import_folder?:string|null;document_parser?:DocumentParser;document_parser_model?:string;document_parser_pdf_prompt?:string;document_parser_image_prompt?:string;conversion_storage?:ConversionStorage;mineru?:MineruConfig;model_proxy?:ModelProxy;runtime?:Record<string,number|boolean>;notes?:NoteSettings;updates?:UpdateSettings;conversion_workers?:number;convert_other_formats?:boolean;conversion_formats?:string[]|null;search_summary_enabled?:boolean;search_summary_model?:string;search_summary_sources?:number;retrieval_version?:number;language?:Language;credential_error?:string|null;answer:ModelConfig;jev:ModelConfig;retrieval:Retrieval;theme:string;start_page:string;editor_size:number;show_hidden:boolean;show_index_details?:boolean;watch:boolean;index_workers?:number;excluded:string[];file_types?:FileTypes;timeout_secs:number;answer_context_tokens:number;answer_output_tokens:number;max_tool_calls?:number;model_profiles?:Record<string,ModelProfile>;mcp_servers?:McpServer[]};
export type ReasoningBlock={answer_offset:number;tool_count:number;content:string};
export type Message={help?:boolean;created_at?:number|null;question:string;answer:string;reasoning?:string;reasoning_blocks?:ReasoningBlock[];evidence:Candidate[];model:string;scope:string[];references:string[];status:string;error:string|null;retrieval_question:string;tool_calls?:ToolCallRecord[];context?:AnswerContext|null;note_path?:string|null};
export type Conversation={id:string;title:string;messages:Message[];updated:number;deleted:boolean};
export type Draft={path:string;source_path?:string;name:string;body:string;hash:string;is_new:boolean};
export type ToolCallRecord={id:string;answer_offset?:number|null;query:string;source_count:number;elapsed_ms?:number|null;tool_name?:string|null;server_name?:string|null;is_error?:boolean;denied?:boolean;skipped?:boolean;result?:unknown;error?:string|null};
export type Progress={reasoning_delta?:string|null;reasoning_block?:ReasoningBlock|null;replace?:boolean;id:string;stage:string;delta:string|null;count:number;tool?:{call:ToolCallRecord;finished:boolean}|null};
export type SearchPagination={total:number;page:number;page_size:number};
export type QueryOutput={terms:string[];elapsed_ms?:number;results:Candidate[];pagination?:SearchPagination|null;trace:{hop:number;candidates:number[];terms:{term:string;score:number}[];evidence:number[]}[];conversation:Conversation|null};

export type CliAccess={enabled:boolean};

export type DoclingProfile='documents'|'media';
export type DoclingStatus={installed:boolean;running:boolean;stage:string;message:string;downloaded:number;total:number|null;percent?:number|null;packages_done?:number;packages_total?:number;error:string|null;formats:[string,string[]][];builtin_formats?:[string,string[]][];profiles?:{id:DoclingProfile;installed:boolean;extensions:string[]}[];requested_profiles?:DoclingProfile[]};

export type ConversionSegment={current:number;total:number;start_page:number;end_page:number};
export type IndexSummary={checked:number;reused:number;updated:number;failed:number};
export type IndexProgress={id?:string;kind?:'primary'|'enhancement';path:string;count:number;total:number;stage?:string;summary?:IndexSummary};
export type ActiveSegment={segment_id:number;start_page:number;end_page:number;engine:string;phase:string};
export type SegmentProgress={total:number;completed:number;active:ActiveSegment[];waiting_local?:number;failed:number;cancelled:number};
export type IndexTask={id?:string;kind?:'primary'|'enhancement';path:string;stage:string;completed:number;total:number;files:string[];summary?:IndexSummary;conversions?:{path:string;engine:string;segment?:ConversionSegment;segment_progress?:SegmentProgress}[]};
export type ConversionState={source:string;root:string;status:string;searchable:boolean;error?:string|null};
export type IndexFailure={path:string;root:string;error:string};
export type IndexStatus={paused:boolean;tasks:IndexTask[];failed_files?:number;failed_documents?:IndexFailure[];conversion_states?:ConversionState[]};
export type FolderIndexOptions={extensions?:string[]|null;file_types:FileTypes;convert_other_formats:boolean;conversion_formats?:string[]|null};
export type IndexHistory={id:number;path:string;name:string;started_at:number;finished_at:number;completed:number;total:number;status:string;error:string|null;summary?:IndexSummary};
export type IndexFileLog={segment?:ConversionSegment;phase?:string;engine?:string;elapsed_ms?:number;time:number;path:string;stage:string;status:string;error:string|null};
export type IndexHistoryLog={raw:string|null};

export type NoteSourceState='intact'|'moved'|'changed'|'missing'|'unverified';
export type NoteSource={id:number;path:string;display_path?:string|null;hash:string;start_byte:number;end_byte:number;start_line:number;end_line:number;text:string};
export type NoteSourceStatus={source:NoteSource;state:NoteSourceState;reason?:string|null;current?:SourceSpan|null;current_hash?:string|null;current_text?:string|null;similarity?:number|null;checked_at?:number|null};
export type NoteSummary={path:string;title:string;question:string;asked_at?:number|null;model?:string;scope?:string[];state:NoteSourceState;source_count:number;attention:number;checked_at?:number|null;parse_error?:string|null};
export type NoteDetail={summary:NoteSummary;sources:NoteSourceStatus[]};
