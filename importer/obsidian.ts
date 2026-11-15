// Headless bindings absent from upstream's web shim. The SAG UI never constructs these.
export * from '../third-party/obsidian-importer/web/obsidian/index';
export class SettingGroup { constructor(){ throw new Error('Importer settings are provided by SAG'); } }
export class ButtonComponent { constructor(){ throw new Error('Importer settings are provided by SAG'); } }
export class MarkdownRenderChild {}
export class MarkdownView {}
export class WorkspaceLeaf {}
export class MarkdownRenderer { static render(){ throw new Error('Importer preview is provided by SAG'); } }
