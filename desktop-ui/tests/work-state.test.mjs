import test from 'node:test';import assert from 'node:assert/strict';import {countdown,pages,statuses,searchErrors} from '../src/work-state.ts';import {readFileSync} from 'node:fs';
test('countdown never goes negative and distinguishes no deadline',()=>{assert.equal(countdown(null,1000),'—');assert.equal(countdown(1001,1000),'00:01');assert.equal(countdown(1000,2000),'00:00');assert.equal(countdown(61000,1000),'01:00');});
test('all browser stages and public errors are translated',()=>{const messages=JSON.parse(readFileSync(new URL('../src/messages.json',import.meta.url)));for(const text of [...Object.values(pages),...Object.values(statuses),...Object.values(searchErrors)])for(const lang of ['en','es'])if(!['CAPTCHA','HTTP 429'].includes(text))assert.ok(messages[lang][text],`${lang}: ${text}`);});

test('budget save errors map any rejection shape to one translatable text',async()=>{
 const {budgetErrorText}=await import('../src/work-state.ts');
 assert.equal(budgetErrorText('budget_in_use'),'Этот бюджет используется другим поиском');
 assert.equal(budgetErrorText(new Error('invalid_target')),'Бюджет должен быть целым числом от 1 до 2 147 483 647.');
 assert.equal(budgetErrorText('profile_not_found'),'Не удалось открыть профиль');
 assert.equal(budgetErrorText(new Error('something odd')),'Не удалось сохранить бюджет на сегодня');
 assert.equal(budgetErrorText(undefined),'Не удалось сохранить бюджет на сегодня');
});

import {promotedControl} from '../src/work-state.ts';
test('the browser button is promoted while the user must act or a stopped search left its browser open (Python 0.3.6 update_state_presentation)',()=>{
 const idle={active:false,manual:false,attention:false,browser_pid:null};
 assert.equal(promotedControl(null),'start');
 assert.equal(promotedControl(idle),'start');
 assert.equal(promotedControl({...idle,active:true}),'start');
 assert.equal(promotedControl({...idle,active:true,manual:true,browser_pid:7}),'browser');
 assert.equal(promotedControl({...idle,active:true,attention:true,browser_pid:7}),'browser');
 assert.equal(promotedControl({...idle,browser_pid:7}),'browser','a browser kept after Stop (retained)');
});

test('a pause right after the start is a second click on the same button, not a request to pause',async()=>{
 const {pauseTooSoon,PAUSE_GRACE_MS}=await import('../src/work-state.ts');
 assert.equal(pauseTooSoon(null,5000),false,'nothing was started from here');
 assert.equal(pauseTooSoon(1000,1000+253),true,'the double click that paused a search 253 ms after its start');
 assert.equal(pauseTooSoon(1000,1000+PAUSE_GRACE_MS-1),true);
 assert.equal(pauseTooSoon(1000,1000+PAUSE_GRACE_MS),false,'a pause after the grace time is meant');
 assert.equal(pauseTooSoon(2000,1000),false,'a clock that went back never blocks');
});

test('after a click on pause or resume the window shows it as taken until the worker has applied it',async()=>{
 const {pendingControl,PENDING_CONTROL_MS}=await import('../src/work-state.ts');
 const running={active:true,paused:false,manual:false},paused={active:true,paused:true,manual:false};
 assert.equal(pendingControl(null,running,5000),null);
 assert.equal(pendingControl({command:'pause',since:1000},running,1500),'pause','not paused yet: the click is still being carried out');
 assert.equal(pendingControl({command:'pause',since:1000},paused,1500),null,'paused: the real state shows');
 assert.equal(pendingControl({command:'resume',since:1000},paused,1500),'resume');
 assert.equal(pendingControl({command:'resume',since:1000},running,1500),null);
 assert.equal(pendingControl({command:'pause',since:1000},{active:false,paused:false,manual:false},1500),null,'the search ended');
 assert.equal(pendingControl({command:'pause',since:1000},{active:true,paused:false,manual:true},1500),null,'the person took the browser');
 assert.equal(pendingControl({command:'pause',since:1000},running,1000+PENDING_CONTROL_MS),null,'never stuck: the real state shows after a while');
 assert.equal(pendingControl({command:'pause',since:1000},null,1500),null);
});
