<script lang="ts">
// The developer window: three clicks on the small wrench in About. Debugging aids for the owner; nothing here shows a
// value the person typed or keeps in a profile.
import {onMount} from 'svelte';import Icon from './Icon.svelte';
import {developerState,developerEnvironment,developerLearning,developerOpen,developerCopyEnvironment,developerExport,developerSiteCheck,type DeveloperState,type DeveloperEnvironment} from './bridge';
let {t,onback}:{t:(key:string)=>string;onback:()=>void}=$props();
let dev=$state<DeveloperState|null>(null);let env=$state<DeveloperEnvironment|null>(null);
let busy=$state(false);let message=$state('');let failed=$state(false);
let checkBrowser=$state('');let checking=$state(false);let check=$state<{result:string;code?:string;seconds:number}|null>(null);
async function refresh(){try{dev=await developerState();}catch{/* preview without the native side */}}
onMount(()=>{void refresh();void (async()=>{try{env=await developerEnvironment();checkBrowser=env.browsers.find(b=>b.support==='supported')?.id??'';}catch{/* preview */}})();const timer=setInterval(()=>void refresh(),1000);return()=>clearInterval(timer);});
async function run(work:()=>Promise<string|void>){busy=true;failed=false;message='';try{message=(await work())??'';}catch{failed=true;}finally{busy=false;}}
const toggle=()=>run(async()=>{dev=await developerLearning(!dev?.learning.on);});
const exportSession=()=>run(async()=>`${t('Файл в «Загрузках»')}: ${await developerExport()}`);
const copyEnvironment=()=>run(async()=>{await developerCopyEnvironment();return t('Скопировано');});
async function siteCheck(){if(!checkBrowser)return;checking=true;check=null;failed=false;try{check=await developerSiteCheck(checkBrowser);}catch{failed=true;}finally{checking=false;}}
const RESULTS:Record<string,string>={opened:'Сайт открылся',certificate_error:'Браузер не проверил сертификат сайта',unreachable:'Сайт не открылся',blocked:'Сайт заблокировал браузер',timeout:'Страница не распознана за 30 с',start_failed:'Браузер не запустился',browser_unsupported:'Этот браузер не поддерживается'};
const dash=(v:unknown)=>v===null||v===undefined||v===''?'—':String(v);
</script>
<div class="developer">
 <div class="developer-head"><button type="button" class="ghost" onclick={onback} aria-label={t('Назад')}><Icon name="back" size={18}/></button><h1>{t('Окно разработчика')}</h1></div>
 <section><h2><Icon name="flask" size={16}/>{t('Запись трека')}</h2>
  <p class="muted">{dev?.learning.on?`${t('Запись включена')}: ${dev.learning.folder}`:t('Запись выключена')}. {t('Записывает структуру страниц и путь между ними, без введённых значений.')}</p>
  <div class="row"><button type="button" class:primary={!dev?.learning.on} disabled={busy||!dev} onclick={()=>void toggle()}><Icon name={dev?.learning.on?'stop':'play'} size={16}/>{t(dev?.learning.on?'Остановить запись':'Начать запись')}</button><button type="button" disabled={busy} onclick={()=>void run(()=>developerOpen('learning'))}><Icon name="folder" size={16}/>{t('Открыть папку')}</button></div>
 </section>
 <section><h2><Icon name="activity" size={16}/>{t('Состояние поиска')}</h2>
  {#if dev?.live}{@const live=dev.live}<dl>
   <dt>{t('Страница')}</dt><dd>{dash(live.page)}{live.variant?` · ${live.variant}`:''}</dd>
   <dt>{t('Фаза')}</dt><dd>{dash(live.flow?.phase)}</dd>
   <dt>{t('Причина остановки')}</dt><dd>{dash(live.flow?.hold_reason)}</dd>
   <dt>{t('Повторы')}</dt><dd>{dash(live.flow?.unsent_retries)} / {dash(live.flow?.recovery_attempts)}</dd>
   <dt>{t('Отказ клика')}</dt><dd>{dash(live.stale?.reason)}</dd>
   <dt>{t('Ввод остановлен')}</dt><dd>{dash(live.entry_stopped)}</dd>
   <dt>{t('Последний шаг')}</dt><dd>{dash(live.last_step?.effects?.join?.(', '))}{live.last_step?.error?` · ${live.last_step.error}`:''}</dd>
  </dl>{:else}<p class="muted">{t('Поиск не идёт')}</p>{/if}
 </section>
 <section><h2><Icon name="folder" size={16}/>{t('Папки и выгрузка')}</h2>
  <div class="row wrap">{#each [['data','Папка данных'],['diagnostics','Диагностика'],['reports','Отчёты']] as [id,label]}<button type="button" disabled={busy} onclick={()=>void run(()=>developerOpen(id as 'data'|'diagnostics'|'reports'))}>{t(label)}</button>{/each}<button type="button" disabled={busy} onclick={()=>void exportSession()}><Icon name="download" size={16}/>{t('Выгрузить последнюю сессию')}</button></div>
 </section>
 <section><h2><Icon name="info" size={16}/>{t('Окружение')}</h2>
  {#if env}<p class="muted">SantaCita {env.app} · {env.system} {env.architecture}</p><ul>{#each env.browsers as b}<li>{b.name} {b.version} <span class="muted">({b.support})</span></li>{/each}</ul>{:else}<p class="muted">…</p>{/if}
  <div class="row"><button type="button" disabled={busy||!env} onclick={()=>void copyEnvironment()}><Icon name="copy" size={16}/>{t('Скопировать')}</button></div>
 </section>
 <section><h2><Icon name="globe" size={16}/>{t('Проверка сайта')}</h2>
  <p class="muted">{t('Открывает стартовую страницу сайта в отдельном временном профиле и закрывает браузер. Дальше стартовой страницы не идёт.')}</p>
  <div class="row"><select bind:value={checkBrowser} disabled={checking||!env}>{#each env?.browsers.filter(b=>b.support==='supported')??[] as b}<option value={b.id}>{b.name}</option>{/each}</select><button type="button" disabled={checking||!checkBrowser||!!dev?.live} onclick={()=>void siteCheck()}><Icon name="search" size={16}/>{t(checking?'Проверяем…':'Проверить')}</button></div>
  {#if dev?.live}<p class="muted">{t('Недоступно, пока идёт поиск')}</p>{/if}
  {#if check}<p role="status">{t(RESULTS[check.result]??check.result)}{check.code?` (${check.code})`:''} · {check.seconds} {t('с')}</p>{/if}
 </section>
 {#if message}<p class="muted" role="status">{message}</p>{/if}{#if failed}<p class="error" role="alert">{t('Не получилось')}</p>{/if}
</div>
<style>.developer{display:flex;flex-direction:column;gap:14px}.developer-head{display:flex;align-items:center;gap:8px}.developer-head h1{font-size:18px;margin:0}
.developer section{display:flex;flex-direction:column;gap:6px}.developer h2{display:flex;align-items:center;gap:6px;font-size:14px;margin:0}
.developer p{margin:0;font-size:12px;line-height:1.5;overflow-wrap:anywhere}.row{display:flex;gap:8px;align-items:center}.row.wrap{flex-wrap:wrap}
.developer dl{display:grid;grid-template-columns:max-content 1fr;gap:2px 12px;margin:0;font-size:12px}.developer dt{color:var(--muted,inherit);opacity:.75}.developer dd{margin:0;overflow-wrap:anywhere;font-family:ui-monospace,monospace}
.developer ul{margin:0;padding-left:18px;font-size:12px}.ghost{background:none;border:none;padding:4px}</style>
