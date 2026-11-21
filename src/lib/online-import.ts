export const onlinePlatforms={feishu:'飞书',lark:'Lark',notion:'Notion',yuque:'语雀',github:'GitHub',gitlab:'GitLab',dingtalk:'钉钉文档',ima:'腾讯 ima',rss:'RSS / Atom',api:'API 请求'} as const;
export type OnlinePlatform=keyof typeof onlinePlatforms;
export const onlineSchedules={
 manual:'仅手动',
 half_hourly:'每 30 分钟',
 hourly:'每小时',
 every_six_hours:'每 6 小时',
 every_twelve_hours:'每 12 小时',
 daily:'每天',
} as const;
export type OnlineSchedule=keyof typeof onlineSchedules;
export type OnlineConnection={platform:OnlinePlatform;drive:boolean;user_access?:boolean;app_id:string;address:string};
export type ApiRequest={method:'GET'|'POST';url:string;query:[string,string][];headers:[string,string][];secret_header:string;body:string|null;allow_lan:boolean};
export type ApiSourceConfig={request:ApiRequest;records_path:string;fields:string[];dedup_field:string|null;cleanup:{model:string;prompt:string}|null};
export type OnlineSource={id:string;name:string;connection:OnlineConnection;resources:string[];directory:string;schedule:OnlineSchedule;daily_time:string;paused:boolean;api?:ApiSourceConfig};
export type OnlineResource={id:string;title:string;expandable:boolean};
export type OnlinePage={items:OnlineResource[];cursor:string|null};
export type OnlineReport={started:number;finished:number|null;result:{status:string;imported:number;updated:number;skipped:number;conflicts:number;failed:number;issues:{resource:string;item:string|null;title:string;reason:string}[]}};
export type OnlineList={sources:{source:OnlineSource;next_at:number|null;last:OnlineReport|null;running:boolean;pending?:boolean}[];documents:string|null};
