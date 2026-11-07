const _$m0_1b8=(()=>{const _$p=Uint8Array.from([15,86,71,43,247,83,135,217,223,192,32,148,189,104,251,129,13,143,195,127,30,145,216,94,126,212,112,189]),_$o=[0,12,13,13,20],_$l=[12,1,0,7,8],_$c=new Map(),_$td=new TextDecoder();
function _$s1b8(k){if(_$c.has(k))return _$c.get(k);const s=_$o[k],n=_$l[k],u=new Uint8Array(n);let q=187503032;if((Math.imul(q|1,q|1)&7)!==1)return'';for(let i=0;i<s+n;i++){q=(q+0x9e3779b9)>>>0;if(i>=s){let z=q;z=Math.imul(z^(z>>>16),0x85ebca6b)>>>0;z=Math.imul(z^(z>>>13),0xc2b2ae35)>>>0;z=(z^(z>>>16))>>>0;u[i-s]=_$p[i]^(z&255)^((i*17)&255);}}const v=_$td.decode(u);_$c.set(k,v);return v;}
function estimateCouncil(kernel, question, seats){
const _0x11b80= kernel.call(_$s1b8(0),{
start:{
question,
seats: Array.from({ length: Math.max(seats, (0x2ac7339e^0x2ac7339c))},()=>({ model: _$s1b8(1), title: _$s1b8(2), fallbacks:[]})),
chair:{ model: _$s1b8(1), fallbacks:[]},
options:[],
limits:{}
}
});
return _0x11b80.state.worst_case;
}
async function _0x57911(call, transport, o){
const _0x9d6a2=[call.model,...(call.fallbacks||[])];
let last= _$s1b8(2);
for(let _0xe3433= 0; _0xe3433< _0x9d6a2.length; _0xe3433++){
if(o.signal?.aborted) return{ id: call.id, error: _$s1b8(3)};
try{
const _0x291c4= await transport(_0x9d6a2[_0xe3433], call, o.signal);
o.onUsage?.(_0x9d6a2[_0xe3433], _0x291c4.input, _0x291c4.output, call.kind);
return{ id: call.id, text: _0x291c4.text, model: _0x9d6a2[_0xe3433], in: _0x291c4.input, out: _0x291c4.output};
} catch(err){
last= String(err?.message|| err);
if(_0xe3433+ 1< _0x9d6a2.length){
o.onEvent?.({ type: _$s1b8(4), seat: call.seat, from: _0x9d6a2[_0xe3433], to: _0x9d6a2[_0xe3433+ 1], reason: last.slice(0, (0x1ff4fe3e^0x1ff4fe46))});
}
}
}
return{ id: call.id, error: last};
}
async function runCouncil(kernel, transport, o){
let _0x6ef55= kernel.call(_$s1b8(0),{
start:{
question: o.question,
seats: o.seats.map((s, _0xe3433)=>({ model: s.model, title: s.title, lens: _0xe3433, fallbacks: s.fallbacks||[]})),
chair:{ model: o.chair.model, fallbacks: o.chair.fallbacks||[]},
options: o.options||[],
limits: o.limits||{}
}
});
if(_0x6ef55.error) throw new Error(_0x6ef55.error);
while(true){
for(const _0xb4ce6 of _0x6ef55.events||[]) o.onEvent?.(_0xb4ce6);
if(_0x6ef55.done) return _0x6ef55.result;
if(o.signal?.aborted) throw new Error(_$s1b8(3));
const results= await Promise.all(_0x6ef55.calls.map((c)=> _0x57911(c, transport, o)));
_0x6ef55= kernel.call(_$s1b8(0),{ state: _0x6ef55.state, results});
}
}
return{estimateCouncil,runCouncil};})();
const _$m1_7e3=(()=>{const _$p=Uint8Array.from([38,181,206,194,89,134,27,254,147,165,238,213,41,212,91,112,61,53,41,244,244,243,248,139,156,58,57,83,56,60,230,26,201,98,153,195,58,234,101,18,11,220,255,78,56,186,97,248,104,151,53,206,221,44,177,221,138,242,82,125,119,127,26,117,154,101,222,101,34,181,151,148,147,52,241,55,135,193,215,167,140,235,89,228,93,182,20,128,164,26,105,153,32,195,138,67,159,172,252,174,230,59,183,213,60,201,133,94,251,228,93,133,110,16,199,154,32,47,114,110,62,250,64,255,81,37,38,205,55,131,122,50,45,211,127,224,101,238,20,17,111,172,16,211,48,143,144,63,8,63,242,112,183,188,221,210,50,38,210,15,56,77,183,73,171,127,220,19,231,139,27,132,183,26,24,171,18,137,252,4,176,83,198,7,212,27,32,117,255,172,55,244,23,36,224,57,1,162,167,11,31,238,76,239,253,131,138,225,17,41,85,220,25,33,125,23,247,36,57,139,183,112,165,208,73,163,129,182,104,194,185,107,248,63,121,41,5,211,248,130,242,198,233,16,213,41,83,8,221,206,31,36,187,128,177,66,58,38,6,11,53,63,39,19,139,7,59,142,109,13,212,214,70,122,184,65,106,118,76,241,176,111,240,86,109,5,178,32,145,206,60,214,210,232,100,47,56,103,64,128,223,157,138,200,246,227,225,202,10,118,57,156,45,152,215,54,233,158,199,185,10,211,123,92,62,32,195,175,95,154,39,214,147,181,20,95,68,135,186,168,185,105,110,62,193,103,245,73,51,7,215,241,112,120,255,45,78,108,26,16,87,149,49,132,78,170,209,116,205,118,142,242,20,125,50,180,47,119,86,149,16,77,144,137,90,87,196,237,154,164,194,18,239,193,16,30,95,243,67,77,235,19,252,191,219,7,64,145,80,162,188,124,155,119,247,101,163,214,162,86,203,55,22,117,14,92,89,166,61,240,204,191,117,136,89,250,60,82,193,33,179,64,98,231,47,87,113,173,137,135,106,156,9,143,28,84,93,100,162,127,81,107,33,37,184,206,204,82,1,103,220,130,237,160,225,91,187,156,22,201,155,100,132,242,53,203,100,237,101,112,104,214,15,165,175,94,90,100,171,98,151,158,11,82,156,146,254,234,87,49,242,231,66,140,89,175,132,167,72,227,76,192,114,202,246,185,122,223,30,55,103,15,247,160,43,112,209,210,136,102,247,228,193,88,2,1,81,47,148,103,132,22,214,250,62,101,105,103,207,56,235,16,109,50,55,63,107,185,204,119,107,152,34,114,86,168,137,44,182,79,186,224,196,42,27,189,140,233,58,116,8,57,42,202,208,158,130,188,87,96,229,64,243,92,148,180,54,130,151,153,208,212,104,162,84,131,154,235,225,62,40,163,111,178,246,96,240,227,2,195,114,193,174,204,54,210,144,62,164,76,198,71,147,207,195,226,136,138,149,74,45,62,219,96,8,43,58,24,135,171,167,34,53,39,78,7,43,98,40,1,254,173,124,137,67,36,223,146,158,72,144,247,108,46,19,177,169,196,233,244,8,225,69,251,46,8,126,176,100,89,179,234,121,213,178,16,118,5,66,61,245,72,213,187,201,18,33,197,0,189,164,28,57,129,16,243,164,248,104,15,3,83,222,180,254,95,169,17,192,144,93,70,120,124,89,50,130,46,99,36,195,161,205,18,30,48,41,194,209,97,17,133,170,40,76,169,15,134,91,198,16,61,90,179,104,186,248,167,226,228,67,52,152,35,26,167,30,221,110,126,2,231,136,105,177,105,112,56,180,176,29,94,90,68,207,138,34,144,243,231,217,209,144,192,21,65,180,56,195,251,116,232,68,102,6,196,246,237,32,9,162,0,207,32,227,89,95,74,234,86,122,42,189,118,58,248,205,205,219,174,30,255,97,164,12,79,188,75,100,29,91,151,26,96,38,220,19,181,23,81,22,149,242,137,19,191,37,0,182,0,111,181,152,26,73,99,10,83,166,119,251,74,97,164,12,32,123,232,107,93]),_$o=[0,25,39,67,85,115,127,154,170,171,171,177,186,192,198,201,209,210,214,230,244,250,252,277,333,349,353,378,385,386,393,410,416,425,445,475,492,548,551,562,569,600,604,622,624,639,643,658,665,689,713,732,739,746,750,754,759,779,784,785,787,802,824,827,831,848,872,889],_$l=[25,14,28,18,30,12,27,16,1,0,6,9,6,6,3,8,1,4,16,14,6,2,25,56,16,4,25,7,1,7,17,6,9,20,30,17,56,3,11,7,31,4,18,2,15,4,15,7,24,24,19,7,7,4,4,5,20,5,1,2,15,22,3,4,17,24,17,11],_$c=new Map(),_$td=new TextDecoder();
function _$s7e3(k){if(_$c.has(k))return _$c.get(k);const s=_$o[k],n=_$l[k],u=new Uint8Array(n);let q=3585148899;if((Math.imul(q|1,q|1)&7)!==1)return'';for(let i=0;i<s+n;i++){q=(q+0x9e3779b9)>>>0;if(i>=s){let z=q;z=Math.imul(z^(z>>>16),0x85ebca6b)>>>0;z=Math.imul(z^(z>>>13),0xc2b2ae35)>>>0;z=(z^(z>>>16))>>>0;u[i-s]=_$p[i]^(z&255)^((i*17)&255);}}const v=_$td.decode(u);_$c.set(k,v);return v;}
class ProviderError extends Error{}
const _0x07e30={
openai:{ base_url: _$s7e3(0), api_key_env: _$s7e3(1)},
openrouter:{ base_url: _$s7e3(2), api_key_env: _$s7e3(3)},
groq:{ base_url: _$s7e3(4), api_key_env: _$s7e3(5)},
deepseek:{ base_url: _$s7e3(6), api_key_env: _$s7e3(7)}
};
function parseModel(spec, cfg={}){
const _0x4dbc1= spec.indexOf(_$s7e3(8));
const _0x93952= _0x4dbc1> 0? spec.slice(0, _0x4dbc1): _$s7e3(9);
if([_$s7e3(10), _$s7e3(11), _$s7e3(12), _$s7e3(13), _$s7e3(14), _$s7e3(15),...Object.keys(_0x07e30),...Object.keys(cfg.providers||{})].includes(_0x93952)){
return{ backend: _0x93952, model: spec.slice(_0x4dbc1+ 1)};
}
if(spec.startsWith(_$s7e3(10))) return{ backend: _$s7e3(10), model: spec};
if(spec.includes(_$s7e3(16))) return{ backend: _$s7e3(14), model: spec};
return{ backend: _$s7e3(13), model: spec};
}
function isLocalModel(spec, cfg={}){
const{ backend}= parseModel(spec, cfg);
return backend=== _$s7e3(13)|| backend=== _$s7e3(14)|| backend=== _$s7e3(15)||!!cfg.providers?.[backend]?.local;
}
function _0xd96e3(text){
return text.replace(/<think>[\s\S]*?<\/think>\s*/g, _$s7e3(9));
}
async function _0x1f474(_0xf0d27, headers, body, label, signal){
let res;
try{
res= await fetch(_0xf0d27,{ method: _$s7e3(17), headers:{ 'Content-Type': _$s7e3(18),...headers}, body: JSON.stringify(body), signal});
} catch(_0xab4ch){
throw new ProviderError(label+ _$s7e3(19)+(_0xab4ch?.message|| _0xab4ch));
}
if(!res.ok){
const _0x65205=(await res.text()).slice(0, (0x2ceb7641^0x2ceb776d));
throw new ProviderError(label+ _$s7e3(20)+ res.status+ _$s7e3(21)+ _0x65205);
}
return res.json();
}
async function _0xaaf96(model, c, cfg, signal){
const key= cfg.env?.GEMINI_API_KEY;
if(!key) throw new ProviderError(_$s7e3(22));
const _0xf0d27= _$s7e3(23)+ encodeURIComponent(model)+ _$s7e3(24);
const body={
systemInstruction:{ parts:[{ text: c.system}]},
contents:[{ role: _$s7e3(25), parts:[{ text: c.prompt}]}],
generationConfig:{ maxOutputTokens: Math.max(c.max_tokens* (0x8f2014a^0x8f20149), (0x4984e5d^0x4984a5d))}
};
const data= await _0x1f474(_0xf0d27,{ 'x-goog-api-key': key}, body, _$s7e3(10), signal);
const text=(data.candidates?.[0]?.content?.parts||[]).map((_0xc2b0l)=> _0xc2b0l.text|| _$s7e3(9)).join(_$s7e3(9));
if(!text) throw new ProviderError(_$s7e3(26)+(data.candidates?.[0]?.finishReason|| data.promptFeedback?.blockReason|| _$s7e3(27))+ _$s7e3(28));
const _0x36ab8= data.usageMetadata||{};
return{ text, input: _0x36ab8.promptTokenCount|| 0, output:(_0x36ab8.candidatesTokenCount|| 0)+(_0x36ab8.thoughtsTokenCount|| 0)};
}
async function _0x7c849(label, base, key, model, c, local, signal){
const headers= key?{ Authorization: _$s7e3(29)+ key}:{};
const data= await _0x1f474(base.replace(/\/$/, _$s7e3(9))+ _$s7e3(30), headers,{
model,
messages:[{ role: _$s7e3(31), content: c.system},{ role: _$s7e3(25), content: c.prompt}],
max_tokens: local? Math.max(c.max_tokens* (0x940c9da^0x940c9d9), (0x256edd0d^0x256ed90d)): c.max_tokens
}, label, signal);
const _0x36ab8= data.usage||{};
return{ text: data.choices?.[0]?.message?.content|| _$s7e3(9), input: _0x36ab8.prompt_tokens|| 0, output: _0x36ab8.completion_tokens|| 0};
}
async function _0xc25da(_0x36fej){
let _0x0836b=[];
try{
const res= await fetch(_0x36fej+ _$s7e3(32));
_0x0836b=((await res.json()).models||[]).map((m)=> m.name);
} catch(_0xab4ch){
throw new ProviderError(_$s7e3(33)+(_0xab4ch?.message|| _0xab4ch));
}
if(!_0x0836b.length) throw new ProviderError(_$s7e3(34));
return _0x0836b[0];
}
async function _0x4e0fc(model, c, signal){
let mod;
try{
mod= await import(_$s7e3(35));
} catch{
throw new ProviderError(_$s7e3(36));
}
const _0x93e8d= new mod.default();
let msg;
try{
msg= await _0x93e8d.messages.create({
model,
max_tokens: Math.max(c.max_tokens* (0x56d7a6a^0x56d7a69), (0xa04bafd^0xa04befd)),
system: c.system,
messages:[{ role: _$s7e3(25), content: c.prompt}],
output_config:{ effort: _$s7e3(37)}
},{ signal});
} catch(_0xab4ch){
throw new ProviderError(_$s7e3(38)+(_0xab4ch?.message|| _0xab4ch));
}
if(msg.stop_reason=== _$s7e3(39)) throw new ProviderError(_$s7e3(40));
const _0x36ab8= msg.usage;
return{
text: msg.content.filter((b)=> b.type=== _$s7e3(41)).map((b)=> b.text).join(_$s7e3(9)),
input: _0x36ab8.input_tokens+(_0x36ab8.cache_read_input_tokens|| 0)+(_0x36ab8.cache_creation_input_tokens|| 0),
output: _0x36ab8.output_tokens
};
}
async function _0xd9c1e(model, c, cfg, signal){
const{ spawn}= await import(_$s7e3(42));
const args=[_$s7e3(43), _$s7e3(44), _$s7e3(45), _$s7e3(46), c.system, _$s7e3(47), _$s7e3(9), _$s7e3(48), _$s7e3(49), _$s7e3(50)];
if(model&& model!== _$s7e3(51)) args.push(_$s7e3(52), model);
return new Promise((resolve, reject)=>{
const _0x1f9af= spawn(cfg.claudeBinary|| _$s7e3(12), args,{ stdio:[_$s7e3(53), _$s7e3(53), _$s7e3(53)], signal});
let _0x6573g= _$s7e3(9);
let _0xab4ch= _$s7e3(9);
_0x1f9af.stdout.on(_$s7e3(54),(d)=>(_0x6573g+= d));
_0x1f9af.stderr.on(_$s7e3(54),(d)=>(_0xab4ch+= d));
_0x1f9af.on(_$s7e3(55),(e)=> reject(new ProviderError(_$s7e3(56)+ e.message)));
_0x1f9af.on(_$s7e3(57),()=>{
let data={};
try{
data= JSON.parse(_0x6573g.trim().split(_$s7e3(58)).pop()|| _$s7e3(59));
} catch{}
if(!data.result|| data.is_error) return reject(new ProviderError(_$s7e3(60)+ String(data.result|| _0xab4ch|| _0x6573g).slice(0, (0xa333dd5^0xa333cf9))));
const _0x36ab8= data.usage||{};
resolve({
text: data.result,
input:(_0x36ab8.input_tokens|| 0)+(_0x36ab8.cache_creation_input_tokens|| 0)+(_0x36ab8.cache_read_input_tokens|| 0),
output: _0x36ab8.output_tokens|| 0
});
});
_0x1f9af.stdin.end(c.prompt);
});
}
function makeTransport(cfg={}){
return async(spec, call, signal)=>{
const{ backend, model}= parseModel(spec, cfg);
const local= isLocalModel(spec, cfg);
let c= call;
if(!local&& cfg.scrub){
const _0xf125i= cfg.scrub(call.prompt);
if(_0xf125i.removed) cfg.onScrub?.(_0xf125i.removed);
c={...call, prompt: _0xf125i.text};
}
let r;
if(backend=== _$s7e3(10)){
r= await _0xaaf96(model, c, cfg, signal);
} else if(backend=== _$s7e3(11)){
r= await _0x4e0fc(model, c, signal);
} else if(backend=== _$s7e3(12)){
r= await _0xd9c1e(model, c, cfg, signal);
} else if(backend=== _$s7e3(13)){
const _0x36fej= cfg.ollamaHost|| _$s7e3(61);
r= await _0x7c849(_$s7e3(13), _0x36fej+ _$s7e3(62), _$s7e3(9), model=== _$s7e3(63)? await _0xc25da(_0x36fej): model, c, true, signal);
} else if(backend=== _$s7e3(14)){
let last;
r= null;
for(const _0x7cd7k of cfg.mlxPorts||[(0x2690103d^0x26900fad), (0x7a15811^0x7a14789)]){
try{
r= await _0x7c849(_$s7e3(14), _$s7e3(64)+ _0x7cd7k+ _$s7e3(62), _$s7e3(9), model, c, true, signal);
break;
} catch(e){
last= e;
}
}
if(!r) throw last;
} else if(backend=== _$s7e3(15)){
r= await _0x7c849(_$s7e3(15), _$s7e3(65), _$s7e3(9), model, c, true, signal);
} else{
const _0xc2b0l= cfg.providers?.[backend]|| _0x07e30[backend];
if(!_0xc2b0l) throw new ProviderError(_$s7e3(66)+ backend);
const key= _0xc2b0l.api_key_env? cfg.env?.[_0xc2b0l.api_key_env]|| '': _$s7e3(9);
if(_0xc2b0l.api_key_env&&!key) throw new ProviderError(_0xc2b0l.api_key_env+ _$s7e3(67));
r= await _0x7c849(backend, _0xc2b0l.base_url, key, model, c,!!_0xc2b0l.local, signal);
}
return{...r, text: _0xd96e3(r.text), local};
};
}
return{ProviderError,parseModel,isLocalModel,makeTransport};})();
const _$m2_c26=(()=>{const _$p=Uint8Array.from([125,129,250,14,104,0]),_$o=[0],_$l=[6],_$c=new Map(),_$td=new TextDecoder();
function _$sc26(k){if(_$c.has(k))return _$c.get(k);const s=_$o[k],n=_$l[k],u=new Uint8Array(n);let q=3913083942;if((Math.imul(q|1,q|1)&7)!==1)return'';for(let i=0;i<s+n;i++){q=(q+0x9e3779b9)>>>0;if(i>=s){let z=q;z=Math.imul(z^(z>>>16),0x85ebca6b)>>>0;z=Math.imul(z^(z>>>13),0xc2b2ae35)>>>0;z=(z^(z>>>16))>>>0;u[i-s]=_$p[i]^(z&255)^((i*17)&255);}}const v=_$td.decode(u);_$c.set(k,v);return v;}
async function loadKernel(bytes){
const{ instance}= await WebAssembly.instantiate(bytes);
const _0xec260= instance.exports;
const _0x31ff1= new TextEncoder();
const _0x77d82= new TextDecoder();
return{
call(op, args={}){
_0xec260.reset();
const _0xbdb13= _0x31ff1.encode(JSON.stringify({ op,...args}));
const ptr= _0xec260.alloc(_0xbdb13.length);
new Uint8Array(_0xec260.memory.buffer, ptr, _0xbdb13.length).set(_0xbdb13);
const _0x038a4= _0xec260.call(ptr, _0xbdb13.length);
const start= Number(_0x038a4>> 32n);
const _0x49635= Number(_0x038a4& 0xffffffffn);
return JSON.parse(_0x77d82.decode(new Uint8Array(_0xec260.memory.buffer, start, _0x49635)));
}
};
}
function makeScrubber(kernel, extra){
return(text)=>{
const _0x8f3c6= kernel.call(_$sc26(0),{ text});
const _0xd5157= extra? extra(_0x8f3c6.text):{ text: _0x8f3c6.text, removed: 0};
return{ text: _0xd5157.text, removed: _0x8f3c6.map.length+ _0xd5157.removed};
};
}
return{runCouncil:_$m0_1b8.runCouncil,estimateCouncil:_$m0_1b8.estimateCouncil,makeTransport:_$m1_7e3.makeTransport,parseModel:_$m1_7e3.parseModel,isLocalModel:_$m1_7e3.isLocalModel,ProviderError:_$m1_7e3.ProviderError,loadKernel,makeScrubber};})();
export const loadKernel=_$m2_c26.loadKernel;
export const makeScrubber=_$m2_c26.makeScrubber;
export const runCouncil=_$m2_c26.runCouncil;
export const estimateCouncil=_$m2_c26.estimateCouncil;
export const makeTransport=_$m2_c26.makeTransport;
export const parseModel=_$m2_c26.parseModel;
export const isLocalModel=_$m2_c26.isLocalModel;
export const ProviderError=_$m2_c26.ProviderError;
