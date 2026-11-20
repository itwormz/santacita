import test from 'node:test';import assert from 'node:assert/strict';
import {allImportant,setAllImportant,IMPORTANT_EVENTS} from '../src/notifications.ts';
test('the four important events are what "all important events" means; the help event is separate',()=>{
 assert.deepEqual(IMPORTANT_EVENTS,['auth','office','booking','error']);
 assert.equal(allImportant(['auth','office','booking','error']),true,'the defaults count as all');
 assert.equal(allImportant(['auth','office','booking']),false);
 assert.equal(allImportant(['auth','office','booking','error','stuck']),true,'the help event does not change it');
});
test('switching "all important events" on or off keeps the help event as the person chose it',()=>{
 assert.deepEqual(setAllImportant([],true),['auth','office','booking','error']);
 assert.deepEqual(setAllImportant(['stuck'],true),['auth','office','booking','error','stuck']);
 assert.deepEqual(setAllImportant(['auth','office','booking','error','stuck'],false),['stuck']);
 assert.deepEqual(setAllImportant(['auth','office','booking','error'],false),[]);
});
