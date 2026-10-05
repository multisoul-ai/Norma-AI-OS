const id = v => typeof v === 'string' && v.length <= 128 && /^[A-Za-z0-9_-]+$/.test(v);
const text = (v, max) => typeof v === 'string' && Buffer.byteLength(v) <= max && !/[\u0000-\u001f\u007f]/u.test(v);
const exact = (v, fields) => v && typeof v === 'object' && !Array.isArray(v) && Object.keys(v).every(k => fields.includes(k));
const count = v => Number.isSafeInteger(v) && v >= 0;
const fail = () => { throw { status: 400, body: { error: 'invalid_event' } }; };
export function validateActivity(input, now) {
  if (!exact(input, ['event_id','token','expires_at','kind','event','timestamp','stale_date','dismissal_date','priority','attributes','content_state'])
      || !/^[a-f0-9]{8}(-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(input.event_id)
      || typeof input.token !== 'string' || !/^(?:[a-f0-9]{2}){16,256}$/.test(input.token)
      || !['start','update','end'].includes(input.event) || ![5,10].includes(input.priority)
      || !Number.isSafeInteger(input.timestamp) || input.timestamp > now || input.timestamp < now-120
      || !Number.isSafeInteger(input.expires_at) || input.expires_at <= now || input.expires_at > now+120
      || input.stale_date !== input.timestamp+120
      || (input.event === 'end' ? input.dismissal_date !== input.timestamp+600 : input.dismissal_date !== undefined)) fail();
  if (input.event === 'start') {
    const a=input.attributes;
    if (!exact(a,['machine_id','desktop_workspace_id','generation','cycle_id','started_at'])
        || typeof a.machine_id !== 'string' || !/^mac-[a-f0-9]{64}$/.test(a.machine_id)
        || ![a.desktop_workspace_id,a.generation,a.cycle_id].every(id) || a.started_at !== input.timestamp) fail();
  } else if (input.attributes !== undefined) fail();
  const s=input.content_state;
  if (!exact(s,['name','updated_at','active','running_count','approval_count','review_count','failure_count','hidden_projects','projects'])
      || !text(s.name,60) || s.updated_at !== input.timestamp || typeof s.active !== 'boolean'
      || !['running_count','approval_count','review_count','failure_count','hidden_projects'].every(k=>count(s[k]))
      || !Array.isArray(s.projects) || s.projects.length>3) fail();
  for (const p of s.projects) {
    if (!exact(p,['area_id','name','status','reason','agents','other_agents']) || !id(p.area_id)
        || !text(p.name,48) || !text(p.reason,120) || !['approval','failed','review','running','idle'].includes(p.status)
        || !count(p.other_agents) || !Array.isArray(p.agents) || p.agents.length>2) fail();
    for (const a of p.agents) if (!exact(a,['name','action']) || !text(a.name,30) || !text(a.action,60)) fail();
  }
  if (Buffer.byteLength(activityPayload(input))>4096) fail();
  return input;
}
export function activityPayload(event) {
  const aps={timestamp:event.timestamp,event:event.event,'content-state':event.content_state,'stale-date':event.stale_date};
  if (event.event === 'start') {
    aps['attributes-type']='WorkspaceActivityAttributes';aps.attributes=event.attributes;
    aps.alert={title:'Norma 工作已开始',body:event.content_state.name};
  }
  if (event.event === 'end') aps['dismissal-date']=event.dismissal_date;
  return JSON.stringify({aps});
}
