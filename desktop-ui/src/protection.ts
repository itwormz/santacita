/** "Защита от бана" fields of the profile editor, in Python 0.3.6 order (`build_appointment_protection`). */
export interface ProtectionField {key:string;title:string;description:string;kind:'flag'|'int'|'timing';unit?:string;source:'protection'|'timings'}
export const protectionFields:ProtectionField[]=[
 {key:'confirm_new_round',title:'Перепроверять наличие сит',description:'Когда плашка «Сит нет» исчезнет, начнётся новый проход. Запрос уйдёт, если плашка больше не появится.',kind:'flag',source:'protection'},
 {key:'daily_target',title:'Базовый бюджет',description:'Используется без адаптации или пока нет адаптивной оценки. Бюджет только на сегодня можно изменить отдельно.',kind:'int',source:'protection'},
 {key:'adaptive_target',title:'Адаптивный бюджет',description:'Бюджет на завтра определяется числом запросов до первого ответа с cod. oper, но составляет не менее пяти попыток. Запрос с этим ответом не учитывается.',kind:'flag',source:'protection'},
 {key:'retry_after_office',title:'Повторить после офиса',description:'Если после показа офиса сайт ответит «Сит нет», можно сделать одну попытку раньше запланированного времени. Чистый проход и паузы по-прежнему обязательны.',kind:'flag',source:'protection'},
 {key:'adaptive_reload_on_429',title:'Замедлять обновление при 429',description:'Каждый новый HTTP 429 на форме заявителя добавляет 30 секунд к интервалу её обновления до конца запуска.',kind:'flag',source:'timings'},
 {key:'distribute_attempts',title:'Распределять попытки',description:'Распределять бюджет по времени дня. Вне окна чистый проход ждёт; «Попробовать сейчас» открывает ближайшее окно раньше.',kind:'flag',source:'protection'},
 {key:'switch_browser_on_block',title:'Менять браузер при блокировке',description:'Если после очистки профиля сайт снова блокирует, следующая сессия идёт в другом установленном браузере. Не помогло и это — поиск возвращается в браузер профиля и ждёт задержку при блокировке.',kind:'flag',source:'timings'},
 {key:'blocked_seconds',title:'Задержка при блокировке',description:'Пауза перед новой сессией, когда очистка профиля (и смена браузера, если она включена) не помогла выйти из блокировки, с',kind:'timing',source:'timings'},
];
/**
 * Warning band of the "Проверка сит" section (`update_search_behavior`): entering data on a no-appointments
 * form or switching off any protection flag but "Распределять попытки" is risky. Message 0 = recommended,
 * 1 = repeated submission, 2 = part of the protection is off.
 */
/** Switches that choose how the search works, not how it protects itself: off is no risk. */
const PREFERENCES=new Set(['distribute_attempts','switch_browser_on_block']);
export function searchRisk(enterData:boolean,flags:Record<string,boolean>):{risk:boolean;message:0|1|2}{
 const disabled=Object.entries(flags).some(([key,on])=>!PREFERENCES.has(key)&&!on);
 const risk=enterData||disabled;
 return {risk,message:disabled?2:risk?1:0};
}
