const _$m0_6ba=(()=>{const _$p=Uint8Array.from([48,243,237,180,230,191,230,192,229,18,235,103,56,255,161,151,146,7,91,142,131,184,215,224,75,12,92,130]),_$o=[0,12,13,13,20],_$l=[12,1,0,7,8],_$c=new Map(),_$td=new TextDecoder();
function _$s6ba(k){if(_$c.has(k))return _$c.get(k);const s=_$o[k],n=_$l[k],u=new Uint8Array(n);let q=1850164922;if((Math.imul(q|1,q|1)&7)!==1)return'';for(let i=0;i<s+n;i++){q=(q+0x9e3779b9)>>>0;if(i>=s){let z=q;z=Math.imul(z^(z>>>16),0x85ebca6b)>>>0;z=Math.imul(z^(z>>>13),0xc2b2ae35)>>>0;z=(z^(z>>>16))>>>0;u[i-s]=_$p[i]^(z&255)^((i*17)&255);}}const v=_$td.decode(u);_$c.set(k,v);return v;}
function estimateCouncil(kernel, question, seats){
const _0x46ba0= kernel.call(_$s6ba(0),{
start:{
question,
seats: Array.from({ length: Math.max(seats, (0x39aef440^0x39aef442))},()=>({ model: _$s6ba(1), title: _$s6ba(2), fallbacks:[]})),
chair:{ model: _$s6ba(1), fallbacks:[]},
options:[],
limits:{}
}
});
return _0x46ba0.state.worst_case;
}
async function _0x8c931(call, transport, o){
const _0xd26c2=[call.model,...(call.fallbacks||[])];
let last= _$s6ba(2);
for(let _0x18453= 0; _0x18453< _0xd26c2.length; _0x18453++){
if(o.signal?.aborted) return{ id: call.id, error: _$s6ba(3)};
try{
const _0x5e1e4= await transport(_0xd26c2[_0x18453], call, o.signal);
o.onUsage?.(_0xd26c2[_0x18453], _0x5e1e4.input, _0x5e1e4.output, call.kind);
return{ id: call.id, text: _0x5e1e4.text, model: _0xd26c2[_0x18453], in: _0x5e1e4.input, out: _0x5e1e4.output};
} catch(err){
last= String(err?.message|| err);
if(_0x18453+ 1< _0xd26c2.length){
o.onEvent?.({ type: _$s6ba(4), seat: call.seat, from: _0xd26c2[_0x18453], to: _0xd26c2[_0x18453+ 1], reason: last.slice(0, (0xe1f0304^0xe1f037c))});
}
}
}
return{ id: call.id, error: last};
}
async function runCouncil(kernel, transport, o){
let _0xa3f75= kernel.call(_$s6ba(0),{
start:{
question: o.question,
seats: o.seats.map((s, _0x18453)=>({ model: s.model, title: s.title, lens: _0x18453, fallbacks: s.fallbacks||[]})),
chair:{ model: o.chair.model, fallbacks: o.chair.fallbacks||[]},
options: o.options||[],
limits: o.limits||{}
}
});
if(_0xa3f75.error) throw new Error(_0xa3f75.error);
while(true){
for(const _0xe9d06 of _0xa3f75.events||[]) o.onEvent?.(_0xe9d06);
if(_0xa3f75.done) return _0xa3f75.result;
if(o.signal?.aborted) throw new Error(_$s6ba(3));
const results= await Promise.all(_0xa3f75.calls.map((c)=> _0x8c931(c, transport, o)));
_0xa3f75= kernel.call(_$s6ba(0),{ state: _0xa3f75.state, results});
}
}
return{estimateCouncil,runCouncil};})();
const _$m1_ab9=(()=>{const _$p=Uint8Array.from([215,83,107,93,169,27,35,158,17,75,36,16,223,15,59,226,139,49,56,241,173,159,1,242,111,139,248,218,14,234,159,0,35,53,131,63,78,237,239,203,90,254,39,196,181,228,31,126,36,132,173,131,74,39,88,72,35,246,91,91,170,222,252,128,98,117,14,15,15,170,219,208,209,169,240,45,181,207,46,151,165,236,9,59,9,227,109,8,2,80,47,5,177,187,14,109,54,245,80,39,122,32,186,75,197,73,214,221,34,176,101,62,217,252,11,49,142,45,156,25,224,163,201,79,45,38,34,147,32,1,105,146,100,1,38,206,249,44,161,248,172,104,218,67,184,107,184,97,120,106,125,195,140,24,49,165,184,121,53,198,81,30,252,60,127,246,204,213,236,154,36,253,151,54,112,81,198,8,200,236,180,64,114,230,186,15,74,160,109,135,225,130,28,111,213,14,111,126,203,122,160,182,224,45,120,188,223,30,143,172,85,63,124,89,14,189,123,109,105,147,225,10,189,68,52,49,189,61,126,200,81,87,247,130,143,234,177,164,184,168,66,136,123,97,216,170,253,204,34,167,156,196,100,27,162,77,33,72,205,68,235,68,39,243,81,134,68,216,218,134,132,90,9,21,24,90,89,4,124,196,33,191,14,55,57,199,253,195,169,133,249,134,159,99,51,167,29,143,107,148,194,244,41,11,186,187,236,181,76,239,216,67,163,20,209,49,158,227,187,3,200,76,29,92,211,198,24,169,108,251,133,21,222,165,245,204,22,237,117,105,76,245,5,211,219,29,237,191,233,190,200,163,116,56,36,54,38,19,17,88,241,109,177,156,217,210,147,234,155,43,23,204,228,142,66,59,107,84,166,49,242,124,23,42,180,156,222,138,149,9,31,96,68,118,45,137,140,121,52,2,201,120,146,210,194,184,17,253,102,146,239,57,84,183,82,166,111,103,251,20,15,187,91,142,212,141,123,80,248,133,205,224,251,161,250,197,225,91,129,161,118,177,60,211,69,164,147,253,190,197,33,37,58,109,41,103,82,220,107,59,60,72,34,127,106,87,175,139,63,20,194,142,10,220,207,140,162,234,50,107,52,0,119,153,57,185,220,166,92,213,171,102,31,133,109,232,10,248,97,182,78,19,120,82,145,172,188,173,100,22,73,238,159,20,179,247,136,163,32,51,150,180,153,102,119,227,168,173,69,141,97,175,216,162,4,178,140,214,36,232,136,171,112,44,28,233,230,230,83,124,114,103,117,75,90,75,193,161,45,27,105,71,150,164,124,224,126,128,146,103,65,247,117,157,27,219,85,47,171,189,38,128,150,200,176,21,141,107,142,93,214,94,19,217,165,169,92,254,168,160,248,101,202,189,145,170,7,144,36,15,195,182,223,45,145,119,157,203,19,172,251,154,199,74,36,247,77,120,120,213,101,125,61,110,99,0,24,248,7,236,40,66,116,214,187,65,154,241,103,130,247,85,231,248,170,160,118,70,228,2,107,115,121,136,145,221,201,122,88,15,29,40,164,182,237,141,131,109,204,3,137,103,72,189,198,163,237,138,104,51,156,40,121,140,102,88,157,24,178,28,175,7,226,35,255,218,253,46,222,182,194,136,216,2,109,170,220,36,90,131,155,34,82,118,219,247,69,92,247,31,159,203,130,131,74,239,123,234,68,86,156,97,68,178,38,188,9,149,36,124,37,140,127,41,162,169,253,151,252,144,253,31,32,181,116,185,151,51,134,248,72,105,18,122,42,170,36,190,82,2,77,43,149,166,101,146,52,123,245,150,124,209,204,64,230,5,88,226,188,190,9,15,86,27,251,68,125,136,204,8,163,129,1,222,40,148,20,36,32,224,109,243,36,116,176,254,120,186,60,69,201,246,96,244,96,28,234,166,102,202,19,33,13,86,132,139,76,137,52,67,35,194,135,15,235,7,174,142,200,204,201,195,179,127,174,38,200,12,205,241,35,117,21,69,233,89,126,109,255,89,228,12,206,254,165,130,77,195,182,138,152,119,37,27,19,21,27,60,234,29,38,28,65,253,181,31]),_$o=[0,25,39,67,85,115,127,154,170,171,171,177,186,192,198,201,209,210,214,230,244,250,252,277,333,349,353,378,385,386,393,410,416,425,445,475,493,552,554,569,573,588,595,619,643,662,669,676,680,684,689,709,714,715,717,732,808,830,833,837,854,878,895],_$l=[25,14,28,18,30,12,27,16,1,0,6,9,6,6,3,8,1,4,16,14,6,2,25,56,16,4,25,7,1,7,17,6,9,20,30,18,59,2,15,4,15,7,24,24,19,7,7,4,4,5,20,5,1,2,15,76,22,3,4,17,24,17,11],_$c=new Map(),_$td=new TextDecoder();
function _$sab9(k){if(_$c.has(k))return _$c.get(k);const s=_$o[k],n=_$l[k],u=new Uint8Array(n);let q=2234809017;if((Math.imul(q|1,q|1)&7)!==1)return'';for(let i=0;i<s+n;i++){q=(q+0x9e3779b9)>>>0;if(i>=s){let z=q;z=Math.imul(z^(z>>>16),0x85ebca6b)>>>0;z=Math.imul(z^(z>>>13),0xc2b2ae35)>>>0;z=(z^(z>>>16))>>>0;u[i-s]=_$p[i]^(z&255)^((i*17)&255);}}const v=_$td.decode(u);_$c.set(k,v);return v;}
class ProviderError extends Error{}
const _0x7ab90={
openai:{ base_url: _$sab9(0), api_key_env: _$sab9(1)},
openrouter:{ base_url: _$sab9(2), api_key_env: _$sab9(3)},
groq:{ base_url: _$sab9(4), api_key_env: _$sab9(5)},
deepseek:{ base_url: _$sab9(6), api_key_env: _$sab9(7)}
};
function parseModel(spec, cfg={}){
const _0xc0921= spec.indexOf(_$sab9(8));
const _0x066b2= _0xc0921> 0? spec.slice(0, _0xc0921): _$sab9(9);
if([_$sab9(10), _$sab9(11), _$sab9(12), _$sab9(13), _$sab9(14), _$sab9(15),...Object.keys(_0x7ab90),...Object.keys(cfg.providers||{})].includes(_0x066b2)){
return{ backend: _0x066b2, model: spec.slice(_0xc0921+ 1)};
}
if(spec.startsWith(_$sab9(10))) return{ backend: _$sab9(10), model: spec};
if(spec.includes(_$sab9(16))) return{ backend: _$sab9(14), model: spec};
return{ backend: _$sab9(13), model: spec};
}
function isLocalModel(spec, cfg={}){
const{ backend}= parseModel(spec, cfg);
return backend=== _$sab9(13)|| backend=== _$sab9(14)|| backend=== _$sab9(15)||!!cfg.providers?.[backend]?.local;
}
function _0x4c443(text){
return text.replace(/<think>[\s\S]*?<\/think>\s*/g, _$sab9(9));
}
async function _0x921d4(_0x63a87, headers, body, label, signal){
let res;
try{
res= await fetch(_0x63a87,{ method: _$sab9(17), headers:{ 'Content-Type': _$sab9(18),...headers}, body: JSON.stringify(body), signal});
} catch(_0x9270f){
throw new ProviderError(label+ _$sab9(19)+(_0x9270f?.message|| _0x9270f));
}
if(!res.ok){
const _0xd7f65=(await res.text()).slice(0, (0x9612243^0x961236f));
throw new ProviderError(label+ _$sab9(20)+ res.status+ _$sab9(21)+ _0xd7f65);
}
return res.json();
}
async function _0x1dcf6(model, c, cfg, signal){
const key= cfg.env?.GEMINI_API_KEY;
if(!key) throw new ProviderError(_$sab9(22));
const _0x63a87= _$sab9(23)+ encodeURIComponent(model)+ _$sab9(24);
const body={
systemInstruction:{ parts:[{ text: c.system}]},
contents:[{ role: _$sab9(25), parts:[{ text: c.prompt}]}],
generationConfig:{ maxOutputTokens: Math.max(c.max_tokens* (0x16c593a0^0x16c593a3), (0x1dd7712f^0x1dd7752f))}
};
const data= await _0x921d4(_0x63a87,{ 'x-goog-api-key': key}, body, _$sab9(10), signal);
const text=(data.candidates?.[0]?.content?.parts||[]).map((_0xa9d4j)=> _0xa9d4j.text|| _$sab9(9)).join(_$sab9(9));
if(!text) throw new ProviderError(_$sab9(26)+(data.candidates?.[0]?.finishReason|| data.promptFeedback?.blockReason|| _$sab9(27))+ _$sab9(28));
const _0xa9818= data.usageMetadata||{};
return{ text, input: _0xa9818.promptTokenCount|| 0, output:(_0xa9818.candidatesTokenCount|| 0)+(_0xa9818.thoughtsTokenCount|| 0)};
}
async function _0xef5a9(label, base, key, model, c, local, signal){
const headers= key?{ Authorization: _$sab9(29)+ key}:{};
const data= await _0x921d4(base.replace(/\/$/, _$sab9(9))+ _$sab9(30), headers,{
model,
messages:[{ role: _$sab9(31), content: c.system},{ role: _$sab9(25), content: c.prompt}],
max_tokens: local? Math.max(c.max_tokens* (0x1766b260^0x1766b263), (0x5b467ef^0x5b463ef)): c.max_tokens
}, label, signal);
const _0xa9818= data.usage||{};
return{ text: data.choices?.[0]?.message?.content|| _$sab9(9), input: _0xa9818.prompt_tokens|| 0, output: _0xa9818.completion_tokens|| 0};
}
async function _0x3533a(_0x1e22h){
let _0x7b0cb=[];
try{
const res= await fetch(_0x1e22h+ _$sab9(32));
_0x7b0cb=((await res.json()).models||[]).map((m)=> m.name);
} catch(_0x9270f){
throw new ProviderError(_$sab9(33)+(_0x9270f?.message|| _0x9270f));
}
if(!_0x7b0cb.length) throw new ProviderError(_$sab9(34));
return _0x7b0cb[0];
}
async function _0xc0e5c(model, c, cfg, signal){
let spawn;
try{
spawn=(await import(_$sab9(35))).spawn;
} catch{
throw new ProviderError(_$sab9(36));
}
const args=[_$sab9(37), _$sab9(38), _$sab9(39), _$sab9(40), c.system, _$sab9(41), _$sab9(9), _$sab9(42), _$sab9(43), _$sab9(44)];
if(model&& model!== _$sab9(45)) args.push(_$sab9(46), model);
return new Promise((resolve, reject)=>{
const _0x06bed= spawn(cfg.claudeBinary|| _$sab9(12), args,{ stdio:[_$sab9(47), _$sab9(47), _$sab9(47)], signal});
let _0x4c97e= _$sab9(9);
let _0x9270f= _$sab9(9);
_0x06bed.stdout.on(_$sab9(48),(d)=>(_0x4c97e+= d));
_0x06bed.stderr.on(_$sab9(48),(d)=>(_0x9270f+= d));
_0x06bed.on(_$sab9(49),(e)=> reject(new ProviderError(_$sab9(50)+ e.message)));
_0x06bed.on(_$sab9(51),()=>{
let data={};
try{
data= JSON.parse(_0x4c97e.trim().split(_$sab9(52)).pop()|| _$sab9(53));
} catch{}
if(!data.result|| data.is_error) return reject(new ProviderError(_$sab9(54)+ String(data.result|| _0x9270f|| _0x4c97e).slice(0, (0x2755bc0f^0x2755bd23))));
const _0xa9818= data.usage||{};
resolve({
text: data.result,
input:(_0xa9818.input_tokens|| 0)+(_0xa9818.cache_creation_input_tokens|| 0)+(_0xa9818.cache_read_input_tokens|| 0),
output: _0xa9818.output_tokens|| 0
});
});
_0x06bed.stdin.end(c.prompt);
});
}
function makeTransport(cfg={}){
return async(spec, call, signal)=>{
const{ backend, model}= parseModel(spec, cfg);
const local= isLocalModel(spec, cfg);
let c= call;
if(!local&& cfg.scrub){
const _0xd849g= cfg.scrub(call.prompt);
if(_0xd849g.removed) cfg.onScrub?.(_0xd849g.removed);
c={...call, prompt: _0xd849g.text};
}
let r;
if(backend=== _$sab9(10)){
r= await _0x1dcf6(model, c, cfg, signal);
} else if(backend=== _$sab9(11)){
throw new ProviderError(_$sab9(55));
} else if(backend=== _$sab9(12)){
r= await _0xc0e5c(model, c, cfg, signal);
} else if(backend=== _$sab9(13)){
const _0x1e22h= cfg.ollamaHost|| _$sab9(56);
r= await _0xef5a9(_$sab9(13), _0x1e22h+ _$sab9(57), _$sab9(9), model=== _$sab9(58)? await _0x3533a(_0x1e22h): model, c, true, signal);
} else if(backend=== _$sab9(14)){
let last;
r= null;
for(const _0x63fbi of cfg.mlxPorts||[(0x1414423f^0x14145daf), (0x2e904c7b^0x2e9053e3)]){
try{
r= await _0xef5a9(_$sab9(14), _$sab9(59)+ _0x63fbi+ _$sab9(57), _$sab9(9), model, c, true, signal);
break;
} catch(e){
last= e;
}
}
if(!r) throw last;
} else if(backend=== _$sab9(15)){
r= await _0xef5a9(_$sab9(15), _$sab9(60), _$sab9(9), model, c, true, signal);
} else{
const _0xa9d4j= cfg.providers?.[backend]|| _0x7ab90[backend];
if(!_0xa9d4j) throw new ProviderError(_$sab9(61)+ backend);
const key= _0xa9d4j.api_key_env? cfg.env?.[_0xa9d4j.api_key_env]|| '': _$sab9(9);
if(_0xa9d4j.api_key_env&&!key) throw new ProviderError(_0xa9d4j.api_key_env+ _$sab9(62));
r= await _0xef5a9(backend, _0xa9d4j.base_url, key, model, c,!!_0xa9d4j.local, signal);
}
return{...r, text: _0x4c443(r.text), local};
};
}
return{ProviderError,parseModel,isLocalModel,makeTransport};})();
const _$m2_12c=(()=>{const _$p=Uint8Array.from([166,254,159,224,250,109]),_$o=[0],_$l=[6],_$c=new Map(),_$td=new TextDecoder();
function _$s12c(k){if(_$c.has(k))return _$c.get(k);const s=_$o[k],n=_$l[k],u=new Uint8Array(n);let q=82313516;if((Math.imul(q|1,q|1)&7)!==1)return'';for(let i=0;i<s+n;i++){q=(q+0x9e3779b9)>>>0;if(i>=s){let z=q;z=Math.imul(z^(z>>>16),0x85ebca6b)>>>0;z=Math.imul(z^(z>>>13),0xc2b2ae35)>>>0;z=(z^(z>>>16))>>>0;u[i-s]=_$p[i]^(z&255)^((i*17)&255);}}const v=_$td.decode(u);_$c.set(k,v);return v;}
async function loadKernel(bytes){
const{ instance}= await WebAssembly.instantiate(bytes);
const _0x012c0= instance.exports;
const _0x47051= new TextEncoder();
const _0x8cde2= new TextDecoder();
return{
call(op, args={}){
_0x012c0.reset();
const _0xd2b73= _0x47051.encode(JSON.stringify({ op,...args}));
const ptr= _0x012c0.alloc(_0xd2b73.length);
new Uint8Array(_0x012c0.memory.buffer, ptr, _0xd2b73.length).set(_0xd2b73);
const _0x18904= _0x012c0.call(ptr, _0xd2b73.length);
const start= Number(_0x18904>> 32n);
const _0x5e695= Number(_0x18904& 0xffffffffn);
return JSON.parse(_0x8cde2.decode(new Uint8Array(_0x012c0.memory.buffer, start, _0x5e695)));
}
};
}
function makeScrubber(kernel, extra){
return(text)=>{
const _0xa4426= kernel.call(_$s12c(0),{ text});
const _0xea1b7= extra? extra(_0xa4426.text):{ text: _0xa4426.text, removed: 0};
return{ text: _0xea1b7.text, removed: _0xa4426.map.length+ _0xea1b7.removed};
};
}
return{runCouncil:_$m0_6ba.runCouncil,estimateCouncil:_$m0_6ba.estimateCouncil,makeTransport:_$m1_ab9.makeTransport,parseModel:_$m1_ab9.parseModel,isLocalModel:_$m1_ab9.isLocalModel,ProviderError:_$m1_ab9.ProviderError,loadKernel,makeScrubber};})();
export const loadKernel=_$m2_12c.loadKernel;
export const makeScrubber=_$m2_12c.makeScrubber;
export const runCouncil=_$m2_12c.runCouncil;
export const estimateCouncil=_$m2_12c.estimateCouncil;
export const makeTransport=_$m2_12c.makeTransport;
export const parseModel=_$m2_12c.parseModel;
export const isLocalModel=_$m2_12c.isLocalModel;
export const ProviderError=_$m2_12c.ProviderError;
