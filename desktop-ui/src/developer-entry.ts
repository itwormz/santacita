// The developer window opens after three clicks on the small wrench in About within a second and a half.
export const DEVELOPER_CLICKS=3;
export const DEVELOPER_WINDOW_MS=1500;
/** The clicks still inside the window after one at `now`, and whether they open the developer window. */
export function developerClick(history:number[],now:number):{history:number[];open:boolean}{
 const recent=[...history.filter(at=>now-at<DEVELOPER_WINDOW_MS&&at<=now),now];
 return recent.length>=DEVELOPER_CLICKS?{history:[],open:true}:{history:recent,open:false};
}
