//! Browser-independent startup policy. No I/O, sleeps, randomness or page writes.
use crate::model::{DispatchOutcome,DocumentId,Observation,PageKind,RequestMethod};
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum PreparationDecision{Continue,WaitUntil(u64),ReloadGet,Hold}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum PreparationError{InvalidTiming,ClockReversed,Overflow,Paused,UnexpectedReceipt}
const IDLE_BUDGET_MS:u64=10_000;
const IDLE_STEP_MS:u64=250;
/// With a readiness signal a wait may end after this share of it (and never before `MIN_FLOOR_MS`).
const FLOOR_PERCENT:u64=25;
const MIN_FLOOR_MS:u64=1_500;
#[derive(Clone)]
enum Phase{Initial,First{document:DocumentId,deadline:u64},ReloadReady{document:DocumentId},AwaitNew{document:DocumentId,deadline:u64},Second{document:DocumentId,deadline:u64},Ready,Held}
#[derive(Clone)]
pub struct SessionPreparation{first:u64,second:u64,load_timeout:u64,phase:Phase,last:Option<u64>,paused:Option<u64>,idle_since:Option<u64>}
impl SessionPreparation{
 fn floor(wait:u64)->u64{(wait*FLOOR_PERCENT/100).max(MIN_FLOOR_MS).min(wait)}
 /// The moment the readiness signal starts to count in the wait that ends at `deadline`.
 fn floor_at(deadline:u64,wait:u64)->u64{deadline.saturating_sub(wait).saturating_add(Self::floor(wait))}
 pub fn new(first_ms:u64,second_ms:u64,load_timeout_ms:u64)->Result<Self,PreparationError>{
  if load_timeout_ms==0{return Err(PreparationError::InvalidTiming);}
  Ok(Self{first:first_ms,second:second_ms,load_timeout:load_timeout_ms,phase:Phase::Initial,last:None,paused:None,idle_since:None})
 }
 fn check(&self,now:u64)->Result<(),PreparationError>{if self.last.is_some_and(|old|now<old){Err(PreparationError::ClockReversed)}else{Ok(())}}
 fn deadline(now:u64,delay:u64)->Result<u64,PreparationError>{now.checked_add(delay).ok_or(PreparationError::Overflow)}
 pub fn pause(&mut self,now:u64)->Result<(),PreparationError>{self.check(now)?;self.last=Some(now);if self.paused.is_none(){self.paused=Some(now);}Ok(())}
 pub fn resume(&mut self,now:u64)->Result<(),PreparationError>{
  self.check(now)?;
  if let Some(paused)=self.paused{
   let delay=now.checked_sub(paused).ok_or(PreparationError::ClockReversed)?;
   let shifted=match self.phase.clone(){Phase::First{document,deadline}=>Phase::First{document,deadline:Self::deadline(deadline,delay)?},Phase::AwaitNew{document,deadline}=>Phase::AwaitNew{document,deadline:Self::deadline(deadline,delay)?},Phase::Second{document,deadline}=>Phase::Second{document,deadline:Self::deadline(deadline,delay)?},other=>other};
   self.phase=shifted;self.paused=None;
   if let Some(since)=self.idle_since{self.idle_since=Some(Self::deadline(since,delay)?);}
  }self.last=Some(now);Ok(())
 }
 pub fn observe(&mut self,page:&Observation,official:bool,now:u64)->Result<PreparationDecision,PreparationError>{
  self.check(now)?;if self.paused.is_some(){return Err(PreparationError::Paused);}
  // Never hold up a booking or reload a form/provider/error page during startup.
  if page.kind==PageKind::Booking||(!matches!(page.kind,PageKind::Loading|PageKind::Unknown)&&(!official||page.kind!=PageKind::Province||page.method!=RequestMethod::Get)){
   self.phase=Phase::Ready;self.last=Some(now);return Ok(PreparationDecision::Continue);
  }
  let (phase,decision)=match self.phase.clone(){
   Phase::Ready=>(Phase::Ready,PreparationDecision::Continue),Phase::Held=>(Phase::Held,PreparationDecision::Hold),
   Phase::Initial if page.kind!=PageKind::Province=>(Phase::Initial,PreparationDecision::Continue),
   Phase::Initial=>{let deadline=Self::deadline(now,self.first)?;(Phase::First{document:page.document.clone(),deadline},PreparationDecision::WaitUntil(deadline))},
   Phase::First{document,deadline:_} if page.kind==PageKind::Province&&document!=page.document=>{let deadline=Self::deadline(now,self.first)?;(Phase::First{document:page.document.clone(),deadline},PreparationDecision::WaitUntil(deadline))},
   Phase::First{deadline,..} if page.kind!=PageKind::Province&&now>=Self::deadline(deadline,self.load_timeout)?=>(Phase::Held,PreparationDecision::Hold),
   Phase::First{document,deadline} if now<deadline||page.kind!=PageKind::Province=>(Phase::First{document,deadline},PreparationDecision::WaitUntil(if now<deadline{deadline}else{Self::deadline(now,500)?})),
   Phase::First{document,..}=>(Phase::ReloadReady{document},PreparationDecision::ReloadGet),
   Phase::ReloadReady{document} if document==page.document&&page.kind==PageKind::Province=>(Phase::ReloadReady{document},PreparationDecision::ReloadGet),
   Phase::ReloadReady{..}=>{let deadline=Self::deadline(now,self.first)?;(Phase::First{document:page.document.clone(),deadline},PreparationDecision::WaitUntil(deadline))},
   Phase::AwaitNew{document:_,deadline} if now>=deadline=>(Phase::Held,PreparationDecision::Hold),
   Phase::AwaitNew{document,deadline} if document==page.document||page.kind!=PageKind::Province=>(Phase::AwaitNew{document,deadline},PreparationDecision::WaitUntil(Self::deadline(now,500)?.min(deadline))),
   Phase::AwaitNew{..}=>{let deadline=Self::deadline(now,self.second)?;(Phase::Second{document:page.document.clone(),deadline},PreparationDecision::WaitUntil(deadline))},
   Phase::Second{document,deadline:_} if page.kind==PageKind::Province&&document!=page.document=>{let deadline=Self::deadline(now,self.second)?;(Phase::Second{document:page.document.clone(),deadline},PreparationDecision::WaitUntil(deadline))},
   Phase::Second{deadline,..} if page.kind!=PageKind::Province&&now>=Self::deadline(deadline,self.load_timeout)?=>(Phase::Held,PreparationDecision::Hold),
   Phase::Second{document,deadline} if now<deadline||page.kind!=PageKind::Province=>(Phase::Second{document,deadline},PreparationDecision::WaitUntil(if now<deadline{deadline}else{Self::deadline(now,500)?})),
   Phase::Second{..}=>(Phase::Ready,PreparationDecision::Continue),
  };
  self.phase=phase;self.last=Some(now);Ok(decision)
 }
 /// Like `observe`, but a finished wait is extended while requests are still in flight, for at most
 /// `IDLE_BUDGET_MS` per wait (Python waits for `networkidle` up to 10 s and carries on regardless).
 pub fn observe_quiet(&mut self,page:&Observation,official:bool,now:u64,quiet:bool)->Result<PreparationDecision,PreparationError>{
  let before=self.clone();
  let waiting=matches!(self.phase,Phase::First{..}|Phase::Second{..})&&official&&page.kind==PageKind::Province&&page.method==RequestMethod::Get;
  let decision=self.observe(page,official,now)?;
  if !waiting||quiet||!matches!(decision,PreparationDecision::ReloadGet|PreparationDecision::Continue){self.idle_since=None;return Ok(decision);}
  let since=before.idle_since.unwrap_or(now);
  if now.saturating_sub(since)>=IDLE_BUDGET_MS{self.idle_since=None;return Ok(decision);}
  let wake=Self::deadline(now,IDLE_STEP_MS)?;
  *self=before;self.idle_since=Some(since);self.last=Some(now);
  Ok(PreparationDecision::WaitUntil(wake))
 }
 pub fn reload_sent(&mut self,outcome:DispatchOutcome,now:u64)->Result<(),PreparationError>{
  self.check(now)?;let Phase::ReloadReady{document}=self.phase.clone()else{return Err(PreparationError::UnexpectedReceipt)};
  self.phase=if outcome==DispatchOutcome::NotDispatched{Phase::Held}else{Phase::AwaitNew{document,deadline:Self::deadline(now,self.load_timeout)?}};
  self.last=Some(now);Ok(())
 }
 /// Whether the driver should look for the readiness signal now: a start wait is running and its floor has passed.
 pub fn wants_signal(&self,now:u64)->bool{
  match &self.phase{
   Phase::First{deadline,..}=>now>=Self::floor_at(*deadline,self.first)&&now<*deadline,
   Phase::Second{deadline,..}=>now>=Self::floor_at(*deadline,self.second)&&now<*deadline,
   _=>false,
  }
 }
 fn signal_wake(&self,now:u64)->Option<u64>{
  let (deadline,wait)=match &self.phase{Phase::First{deadline,..}=>(*deadline,self.first),Phase::Second{deadline,..}=>(*deadline,self.second),_=>return None};
  let early=Self::floor_at(deadline,wait);
  if now<early{Some(early)}else if now<deadline{Some(now.saturating_add(IDLE_STEP_MS))}else{None}
 }
 /// Like `observe_quiet`, and a start wait also ends once `ready` (the page's own checks are done) and the network
 /// is quiet, but not before its floor. Without the signal the whole wait is kept. While the wait runs, the next
 /// look comes at the floor and then every `IDLE_STEP_MS`, so the signal can be asked for.
 pub fn observe_ready(&mut self,page:&Observation,official:bool,now:u64,quiet:bool,ready:bool)->Result<PreparationDecision,PreparationError>{
  self.check(now)?;if self.paused.is_some(){return Err(PreparationError::Paused);}
  let waiting=official&&page.kind==PageKind::Province&&page.method==RequestMethod::Get;
  if waiting&&quiet&&ready{
   let (first,second)=(self.first,self.second);
   match &mut self.phase{
    Phase::First{document,deadline} if *document==page.document&&now>=Self::floor_at(*deadline,first)=>*deadline=(*deadline).min(now),
    Phase::Second{document,deadline} if *document==page.document&&now>=Self::floor_at(*deadline,second)=>*deadline=(*deadline).min(now),
    _=>{}
   }
  }
  let decision=self.observe_quiet(page,official,now,quiet)?;
  if waiting&&let PreparationDecision::WaitUntil(until)=decision&&let Some(wake)=self.signal_wake(now){return Ok(PreparationDecision::WaitUntil(until.min(wake)));}
  Ok(decision)
 }
}
