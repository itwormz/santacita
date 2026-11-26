import test from 'node:test';import assert from 'node:assert/strict';
import {developerClick} from '../src/developer-entry.ts';
test('three clicks within a second and a half open the developer window, slower ones do not',()=>{
 let state=developerClick([],0);assert.equal(state.open,false);
 state=developerClick(state.history,400);assert.equal(state.open,false);
 state=developerClick(state.history,900);assert.equal(state.open,true);assert.deepEqual(state.history,[]);
 // Spread out: each click falls outside the window of the first.
 let slow=developerClick([],0);slow=developerClick(slow.history,1000);slow=developerClick(slow.history,2000);assert.equal(slow.open,false);
 slow=developerClick(slow.history,2400);assert.equal(slow.open,true);
});
