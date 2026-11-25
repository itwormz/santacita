import test from 'node:test';import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
const read=path=>readFileSync(new URL(path,import.meta.url),'utf8').trim();
test('every .nvmrc names a Node version the frontend accepts (package.json engines)',()=>{
 const range=JSON.parse(read('../../desktop-ui/package.json')).engines.node;
 const [,low,high]=/^>=(\d+) <(\d+)$/.exec(range);
 for(const file of ['../../.nvmrc','../../desktop-ui/.nvmrc']){
  const major=Number(read(file).split('.')[0]);
  assert.ok(major>=Number(low)&&major<Number(high),`${file} is ${read(file)}, engines allow ${range}`);
 }
});
