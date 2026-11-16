import type {IncomingMessage,ServerResponse} from 'node:http';
export function documentFixtureMiddleware(fixturePath:string|undefined):(request:IncomingMessage,response:ServerResponse,next?:()=>void)=>Promise<void>;
