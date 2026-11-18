/** What the About window shows for a saved report: when it was saved (ids start with the UTC time) and its short number. */
const ID=/^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})-([0-9a-f]{8})$/;
export function reportLabel(id:string,language:string,timeZone?:string):string{
 const match=ID.exec(id);
 if(!match)return `CEH-${id.split('-').pop()??id}`;
 const [,year,month,day,hour,minute,second,short]=match;
 const saved=new Date(Date.UTC(+year,+month-1,+day,+hour,+minute,+second));
 const time=new Intl.DateTimeFormat(language,{day:'numeric',month:'short',hour:'2-digit',minute:'2-digit',hourCycle:'h23',...(timeZone?{timeZone}:{})}).format(saved);
 return `${time} · CEH-${short}`;
}
