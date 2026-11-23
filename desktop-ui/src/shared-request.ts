// One request per key at a time: the window asks for the same statistics from several places every few seconds,
// and an answer that takes longer than the interval must not be asked for again while it is still being computed.
const pending=new Map<string,Promise<unknown>>();
export function sharedRequest<T>(key:string,run:()=>Promise<T>):Promise<T>{
 const found=pending.get(key);if(found)return found as Promise<T>;
 const request=run().finally(()=>{pending.delete(key);});
 pending.set(key,request);return request;
}
