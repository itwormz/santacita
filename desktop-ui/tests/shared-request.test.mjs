import test from 'node:test';import assert from 'node:assert/strict';
import {sharedRequest} from '../src/shared-request.ts';
test('asks for the same key that arrive while one is running share its answer',async()=>{
 let runs=0;let finish;const wait=new Promise(resolve=>finish=resolve);
 const run=()=>{runs++;return wait.then(()=>'answer');};
 const first=sharedRequest('statistics:a',run);const second=sharedRequest('statistics:a',run);
 assert.equal(first,second);finish();
 assert.equal(await first,'answer');assert.equal(runs,1);
});
test('a finished request is asked again, and other keys run on their own',async()=>{
 let runs=0;const run=async()=>++runs;
 assert.equal(await sharedRequest('k',run),1);
 assert.equal(await sharedRequest('k',run),2);
 const [a,b]=await Promise.all([sharedRequest('x',run),sharedRequest('y',run)]);
 assert.notEqual(a,b);assert.equal(runs,4);
});
test('a failed request does not stay in the table',async()=>{
 let runs=0;
 await assert.rejects(sharedRequest('f',async()=>{runs++;throw new Error('native');}));
 assert.equal(await sharedRequest('f',async()=>{runs++;return 'ok';}),'ok');assert.equal(runs,2);
});
