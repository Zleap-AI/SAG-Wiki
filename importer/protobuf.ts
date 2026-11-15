import * as messages from './generated-protobuf.js';
import {descriptor} from '../third-party/obsidian-importer/src/formats/apple-notes/descriptor';

// The upstream converter compares formatting runs using $type.fieldsArray.
// Static protobuf classes omit reflection; attach the schema's field metadata
// to their prototypes, including nested message classes, without runtime eval.
function attachTypes(schema:any,namespace:any){
 for(const [name,type] of Object.entries<any>(schema.nested??{})){
  const value=namespace[name];
  if(type.fields)Object.defineProperty(value.prototype,'$type',{value:{fieldsArray:Object.keys(type.fields).map(name=>({name}))}});
  if(type.nested)attachTypes(type,value);
 }
}
attachTypes(descriptor,messages);

// Static decoders keep conversion compatible with the desktop's strict CSP.
export class Root {
 static fromJSON(){return new Root();}
 lookupType(name:string){
  let value:any=messages;
  for(const part of name.replace(/^\./,'').split('.'))value=value[part];
  if(!value?.decode)throw new Error(`Unknown Notes protobuf: ${name}`);
  return value;
 }
}
