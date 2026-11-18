import test from 'node:test';import assert from 'node:assert/strict';
import {reportLabel} from '../src/report-label.ts';
test('the report label shows the saved time in the local zone and the short number',()=>{
 const label=reportLabel('20261118T094201-804eb1f9','ru','UTC');
 assert.match(label,/18/);assert.match(label,/09:42/);assert.match(label,/CEH-804eb1f9$/);
 assert.match(reportLabel('20261118T094201-804eb1f9','en','UTC'),/Nov/);
 assert.match(reportLabel('20261118T094201-804eb1f9','ru','Europe/Madrid'),/10:42/);
});
test('an id that is not a report id is shown as it is, without a time',()=>{
 assert.equal(reportLabel('weird','ru','UTC'),'CEH-weird');
 assert.equal(reportLabel('2026XX18T094201-804eb1f9','en','UTC'),'CEH-804eb1f9');
});
