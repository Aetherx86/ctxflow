const _$m0_d3b=(()=>{const _$p=Uint8Array.from([148,210,236,28,60,116,92,172,10,28,56,90,206,208,241,27,64,43,183,157,188,115,162,240,210,183,161,5]),_$o=[0,12,13,13,20],_$l=[12,1,0,7,8],_$c=new Map(),_$td=new TextDecoder();
function _$sd3b(k){if(_$c.has(k))return _$c.get(k);const s=_$o[k],n=_$l[k],u=new Uint8Array(n);let q=1207611;if((Math.imul(q|1,q|1)&7)!==1)return'';for(let i=0;i<s+n;i++){q=(q+0x9e3779b9)>>>0;if(i>=s){let z=q;z=Math.imul(z^(z>>>16),0x85ebca6b)>>>0;z=Math.imul(z^(z>>>13),0xc2b2ae35)>>>0;z=(z^(z>>>16))>>>0;u[i-s]=_$p[i]^(z&255)^((i*17)&255);}}const v=_$td.decode(u);_$c.set(k,v);return v;}
function estimateCouncil(kernel, question, seats){
const _0x6d3b0= kernel.call(_$sd3b(0),{
start:{
question,
seats: Array.from({ length: Math.max(seats, (0xb259497^0xb259495))},()=>({ model: _$sd3b(1), title: _$sd3b(2), fallbacks:[]})),
chair:{ model: _$sd3b(1), fallbacks:[]},
options:[],
limits:{}
}
});
return _0x6d3b0.state.worst_case;
}
async function _0xb3141(call, transport, o){
const _0xf8ed2=[call.model,...(call.fallbacks||[])];
let last= _$sd3b(2);
for(let _0x3ec63= 0; _0x3ec63< _0xf8ed2.length; _0x3ec63++){
if(o.signal?.aborted) return{ id: call.id, error: _$sd3b(3)};
try{
const _0x849f4= await transport(_0xf8ed2[_0x3ec63], call, o.signal);
o.onUsage?.(_0xf8ed2[_0x3ec63], _0x849f4.input, _0x849f4.output, call.kind);
return{ id: call.id, text: _0x849f4.text, model: _0xf8ed2[_0x3ec63], in: _0x849f4.input, out: _0x849f4.output};
} catch(err){
last= String(err?.message|| err);
if(_0x3ec63+ 1< _0xf8ed2.length){
o.onEvent?.({ type: _$sd3b(4), seat: call.seat, from: _0xf8ed2[_0x3ec63], to: _0xf8ed2[_0x3ec63+ 1], reason: last.slice(0, (0x40f60c9^0x40f60b1))});
}
}
}
return{ id: call.id, error: last};
}
async function runCouncil(kernel, transport, o){
let _0xca785= kernel.call(_$sd3b(0),{
start:{
question: o.question,
seats: o.seats.map((s, _0x3ec63)=>({ model: s.model, title: s.title, lens: _0x3ec63, fallbacks: s.fallbacks||[]})),
chair:{ model: o.chair.model, fallbacks: o.chair.fallbacks||[]},
options: o.options||[],
limits: o.limits||{}
}
});
if(_0xca785.error) throw new Error(_0xca785.error);
while(true){
for(const _0x10516 of _0xca785.events||[]) o.onEvent?.(_0x10516);
if(_0xca785.done) return _0xca785.result;
if(o.signal?.aborted) throw new Error(_$sd3b(3));
const results= await Promise.all(_0xca785.calls.map((c)=> _0xb3141(c, transport, o)));
_0xca785= kernel.call(_$sd3b(0),{ state: _0xca785.state, results});
}
}
return{estimateCouncil,runCouncil};})();
const _$m1_752=(()=>{const _$p=Uint8Array.from([48,153,97,159,189,191,177,103,22,73,77,200,239,18,82,250,142,132,112,75,175,158,152,10,81,189,156,243,157,226,164,5,14,87,3,112,51,19,250,224,190,40,154,47,203,205,136,233,189,227,201,62,238,166,161,137,158,140,129,97,77,218,65,169,132,75,88,16,126,131,30,164,18,59,186,235,18,186,229,217,183,176,34,129,36,52,109,123,222,89,196,99,110,88,106,188,24,5,91,93,181,28,81,76,98,135,216,201,201,92,165,211,195,227,57,177,219,191,225,124,104,15,21,55,94,0,26,51,61,75,243,52,157,47,109,219,243,199,191,8,94,15,9,111,243,11,23,21,164,63,114,192,165,118,198,67,168,173,74,100,163,198,225,114,227,96,33,131,203,93,176,254,46,169,217,41,110,66,4,234,95,95,39,109,131,112,12,221,199,24,246,166,23,71,216,188,42,205,195,17,240,55,208,31,199,49,176,20,192,51,42,56,188,31,174,54,14,205,15,79,107,22,253,218,225,218,253,207,155,157,154,67,199,235,168,30,60,55,108,184,207,113,22,166,36,130,127,22,219,13,4,124,215,199,221,90,71,70,72,37,128,128,29,138,18,166,170,113,222,254,181,19,2,116,92,162,50,189,196,10,19,50,181,182,121,85,225,244,19,162,16,67,192,12,105,189,134,8,6,47,41,197,67,135,7,56,31,154,245,162,206,42,10,17,232,176,193,53,16,42,247,98,141,181,28,209,179,201,11,228,224,119,111,58,223,56,2,148,178,245,215,83,99,104,61,146,159,56,185,107,46,82,118,66,173,58,12,30,189,86,236,180,199,43,154,99,93,241,75,42,79,184,179,237,219,230,104,214,180,93,64,154,236,58,162,12,149,252,93,156,219,2,139,184,9,183,112,92,67,51,74,206,163,202,171,65,42,205,184,255,64,233,190,253,145,25,112,15,107,149,101,115,116,185,109,238,251,67,1,72,147,183,209,179,234,61,75,155,16,180,205,33,122,116,162,15,118,172,244,108,86,73,76,236,111,204,50,73,17,230,125,168,26,18,159,235,190,67,242,39,187,113,210,117,250,49,84,245,89,24,231,111,163,215,236,50,37,221,207,74,208,116,170,94,120,98,6,3,13,229,71,245,128,39,11,78,161,70,117,78,53,143,176,182,243,248,13,250,42,143,237,146,251,18,98,38,46,179,41,61,154,3,89,114,4,77,189,129,103,235,35,151,141,246,145,227,255,254,89,71,154,108,203,140,194,2,112,40,20,224,248,136,198,70,203,202,136,235,18,42,175,47,0,194,105,147,22,156,3,40,158,106,251,153,116,223,255,201,234,143,122,110,235,76,108,168,230,123,170,100,152,33,247,109,253,159,84,24,110,89,87,227,45,227,198,146,221,100,156,217,25,184,213,197,233,131,228,255,130,10,110,209,114,118,154,9,145,124,143,162,196,167,219,187,48,153,134,158,27,133,90,202,35,70,86,217,113,207,226,143,41,166,135,30,89,5,182,167,114,16,15,39,217,163,187,252,207,44,71,133,11,13,40,34,1,207,185,6,203,135,142,15,164,170,235,171,215,199,102,226,223,192,229,160,149,98,148,232,81,93,163,187,215,189,253,156,98,195,139,246,18,219,34,159,150,37,244,239,69,153,187,240,36,4,95,58,23,160,135,43,202,37,56,36,103,104,202,52,117,126,47,5,219,18,124,48,153,237,185,116,41,142,128,96,249,145,150,203,47,99,109,79,238,107,187,213,201,113,145,17,62,138,190,204,219,132,58,83,9,42,80,185,50,241,204,49,15,115,101,89,222,253,66,22,125,117,207,45,115,191,146,58,101,229,228,215,203,96,219,213,80,166,56,90,73,243,116,80,70,74,34,146,246,143,117,116,15,173,105,38,34,64,70,130,182,205,216,248,52,7,205,125,102,163,60,23,127,168,22,35,197,6,155,28,119,119,119,164,169,253,216,3,174,237,238,46,174,254,252,75,133,31,165,240,67,162,209,16,195,174,148,162,3,71,206,113,58,218,254,197,235,26,34,188,159,19,232,138,29,13,28,209,13,89,173,44,216,34,31,186,180,229,35,247,158]),_$o=[0,25,39,67,85,115,127,154,170,171,171,177,186,192,198,201,209,210,214,230,244,250,252,277,333,349,353,378,385,386,393,410,416,425,445,475,476,494,512,571,573,588,592,607,614,638,662,681,688,695,699,703,708,728,733,734,736,751,827,849,852,856,873,897,914],_$l=[25,14,28,18,30,12,27,16,1,0,6,9,6,6,3,8,1,4,16,14,6,2,25,56,16,4,25,7,1,7,17,6,9,20,30,1,18,18,59,2,15,4,15,7,24,24,19,7,7,4,4,5,20,5,1,2,15,76,22,3,4,17,24,17,11],_$c=new Map(),_$td=new TextDecoder();
function _$s752(k){if(_$c.has(k))return _$c.get(k);const s=_$o[k],n=_$l[k],u=new Uint8Array(n);let q=3741206354;if((Math.imul(q|1,q|1)&7)!==1)return'';for(let i=0;i<s+n;i++){q=(q+0x9e3779b9)>>>0;if(i>=s){let z=q;z=Math.imul(z^(z>>>16),0x85ebca6b)>>>0;z=Math.imul(z^(z>>>13),0xc2b2ae35)>>>0;z=(z^(z>>>16))>>>0;u[i-s]=_$p[i]^(z&255)^((i*17)&255);}}const v=_$td.decode(u);_$c.set(k,v);return v;}
class ProviderError extends Error{}
const _0x47520={
openai:{ base_url: _$s752(0), api_key_env: _$s752(1)},
openrouter:{ base_url: _$s752(2), api_key_env: _$s752(3)},
groq:{ base_url: _$s752(4), api_key_env: _$s752(5)},
deepseek:{ base_url: _$s752(6), api_key_env: _$s752(7)}
};
function parseModel(spec, cfg={}){
const _0x8d2b1= spec.indexOf(_$s752(8));
const _0xd3042= _0x8d2b1> 0? spec.slice(0, _0x8d2b1): _$s752(9);
if([_$s752(10), _$s752(11), _$s752(12), _$s752(13), _$s752(14), _$s752(15),...Object.keys(_0x47520),...Object.keys(cfg.providers||{})].includes(_0xd3042)){
return{ backend: _0xd3042, model: spec.slice(_0x8d2b1+ 1)};
}
if(spec.startsWith(_$s752(10))) return{ backend: _$s752(10), model: spec};
if(spec.includes(_$s752(16))) return{ backend: _$s752(14), model: spec};
return{ backend: _$s752(13), model: spec};
}
function isLocalModel(spec, cfg={}){
const{ backend}= parseModel(spec, cfg);
return backend=== _$s752(13)|| backend=== _$s752(14)|| backend=== _$s752(15)||!!cfg.providers?.[backend]?.local;
}
function _0x18dd3(text){
return text.replace(/<think>[\s\S]*?<\/think>\s*/g, _$s752(9));
}
async function _0x5eb64(_0x30417, headers, body, label, signal){
let res;
try{
res= await fetch(_0x30417,{ method: _$s752(17), headers:{ 'Content-Type': _$s752(18),...headers}, body: JSON.stringify(body), signal});
} catch(_0x5f09f){
throw new ProviderError(label+ _$s752(19)+(_0x5f09f?.message|| _0x5f09f));
}
if(!res.ok){
const _0xa48f5=(await res.text()).slice(0, (0x1a538766^0x1a53864a));
throw new ProviderError(label+ _$s752(20)+ res.status+ _$s752(21)+ _0xa48f5);
}
return res.json();
}
async function _0xea686(model, c, cfg, signal){
const key= cfg.env?.GEMINI_API_KEY;
if(!key) throw new ProviderError(_$s752(22));
const _0x30417= _$s752(23)+ encodeURIComponent(model)+ _$s752(24);
const body={
systemInstruction:{ parts:[{ text: c.system}]},
contents:[{ role: _$s752(25), parts:[{ text: c.prompt}]}],
generationConfig:{ maxOutputTokens: Math.max(c.max_tokens* (0x34a1c6e7^0x34a1c6e4), (0x292f7d8e^0x292f798e))}
};
const data= await _0x5eb64(_0x30417,{ 'x-goog-api-key': key}, body, _$s752(10), signal);
const text=(data.candidates?.[0]?.content?.parts||[]).map((_0x766dj)=> _0x766dj.text|| _$s752(9)).join(_$s752(9));
if(!text) throw new ProviderError(_$s752(26)+(data.candidates?.[0]?.finishReason|| data.promptFeedback?.blockReason|| _$s752(27))+ _$s752(28));
const _0x761a8= data.usageMetadata||{};
return{ text, input: _0x761a8.promptTokenCount|| 0, output:(_0x761a8.candidatesTokenCount|| 0)+(_0x761a8.thoughtsTokenCount|| 0)};
}
async function _0xbbf39(label, base, key, model, c, local, signal){
const headers= key?{ Authorization: _$s752(29)+ key}:{};
const data= await _0x5eb64(base.replace(/\/$/, _$s752(9))+ _$s752(30), headers,{
model,
messages:[{ role: _$s752(31), content: c.system},{ role: _$s752(25), content: c.prompt}],
max_tokens: local? Math.max(c.max_tokens* (0x2eba8573^0x2eba8570), (0x27d2c6f2^0x27d2c2f2)): c.max_tokens
}, label, signal);
const _0x761a8= data.usage||{};
return{ text: data.choices?.[0]?.message?.content|| _$s752(9), input: _0x761a8.prompt_tokens|| 0, output: _0x761a8.completion_tokens|| 0};
}
async function _0x01cca(_0xeabbh){
let _0x47a5b=[];
try{
const res= await fetch(_0xeabbh+ _$s752(32));
_0x47a5b=((await res.json()).models||[]).map((m)=> m.name);
} catch(_0x5f09f){
throw new ProviderError(_$s752(33)+(_0x5f09f?.message|| _0x5f09f));
}
if(!_0x47a5b.length) throw new ProviderError(_$s752(34));
return _0x47a5b[0];
}
async function _0x8d7ec(model, c, cfg, signal){
if(model.startsWith(_$s752(35))|| /[\s\x00-\x1f]/.test(model)) throw new ProviderError(_$s752(36));
let spawn;
try{
spawn=(await import(_$s752(37))).spawn;
} catch{
throw new ProviderError(_$s752(38));
}
const args=[_$s752(39), _$s752(40), _$s752(41), _$s752(42), c.system, _$s752(43), _$s752(9), _$s752(44), _$s752(45), _$s752(46)];
if(model&& model!== _$s752(47)) args.push(_$s752(48), model);
return new Promise((resolve, reject)=>{
const _0xd357d= spawn(cfg.claudeBinary|| _$s752(12), args,{ stdio:[_$s752(49), _$s752(49), _$s752(49)], signal});
let _0x1930e= _$s752(9);
let _0x5f09f= _$s752(9);
_0xd357d.stdout.on(_$s752(50),(d)=>(_0x1930e+= d));
_0xd357d.stderr.on(_$s752(50),(d)=>(_0x5f09f+= d));
_0xd357d.on(_$s752(51),(e)=> reject(new ProviderError(_$s752(52)+ e.message)));
_0xd357d.on(_$s752(53),()=>{
let data={};
try{
data= JSON.parse(_0x1930e.trim().split(_$s752(54)).pop()|| _$s752(55));
} catch{}
if(!data.result|| data.is_error) return reject(new ProviderError(_$s752(56)+ String(data.result|| _0x5f09f|| _0x1930e).slice(0, (0x2e0894e0^0x2e0895cc))));
const _0x761a8= data.usage||{};
resolve({
text: data.result,
input:(_0x761a8.input_tokens|| 0)+(_0x761a8.cache_creation_input_tokens|| 0)+(_0x761a8.cache_read_input_tokens|| 0),
output: _0x761a8.output_tokens|| 0
});
});
_0xd357d.stdin.end(c.prompt);
});
}
function makeTransport(cfg={}){
return async(spec, call, signal)=>{
const{ backend, model}= parseModel(spec, cfg);
const local= isLocalModel(spec, cfg);
let c= call;
if(!local&& cfg.scrub){
const _0xa4e2g= cfg.scrub(call.prompt);
if(_0xa4e2g.removed) cfg.onScrub?.(_0xa4e2g.removed);
c={...call, prompt: _0xa4e2g.text};
}
let r;
if(backend=== _$s752(10)){
r= await _0xea686(model, c, cfg, signal);
} else if(backend=== _$s752(11)){
throw new ProviderError(_$s752(57));
} else if(backend=== _$s752(12)){
r= await _0x8d7ec(model, c, cfg, signal);
} else if(backend=== _$s752(13)){
const _0xeabbh= cfg.ollamaHost|| _$s752(58);
r= await _0xbbf39(_$s752(13), _0xeabbh+ _$s752(59), _$s752(9), model=== _$s752(60)? await _0x01cca(_0xeabbh): model, c, true, signal);
} else if(backend=== _$s752(14)){
let last;
r= null;
for(const _0x3094i of cfg.mlxPorts||[(0x3b6ae9c6^0x3b6af656), (0x1fdee260^0x1fdefdf8)]){
try{
r= await _0xbbf39(_$s752(14), _$s752(61)+ _0x3094i+ _$s752(59), _$s752(9), model, c, true, signal);
break;
} catch(e){
last= e;
}
}
if(!r) throw last;
} else if(backend=== _$s752(15)){
r= await _0xbbf39(_$s752(15), _$s752(62), _$s752(9), model, c, true, signal);
} else{
const _0x766dj= cfg.providers?.[backend]|| _0x47520[backend];
if(!_0x766dj) throw new ProviderError(_$s752(63)+ backend);
const key= _0x766dj.api_key_env? cfg.env?.[_0x766dj.api_key_env]|| '': _$s752(9);
if(_0x766dj.api_key_env&&!key) throw new ProviderError(_0x766dj.api_key_env+ _$s752(64));
r= await _0xbbf39(backend, _0x766dj.base_url, key, model, c,!!_0x766dj.local, signal);
}
return{...r, text: _0x18dd3(r.text), local};
};
}
return{ProviderError,parseModel,isLocalModel,makeTransport};})();
const _$m2_431=(()=>{const _$p=Uint8Array.from([204,186,113,91,132,13]),_$o=[0],_$l=[6],_$c=new Map(),_$td=new TextDecoder();
function _$s431(k){if(_$c.has(k))return _$c.get(k);const s=_$o[k],n=_$l[k],u=new Uint8Array(n);let q=4044031025;if((Math.imul(q|1,q|1)&7)!==1)return'';for(let i=0;i<s+n;i++){q=(q+0x9e3779b9)>>>0;if(i>=s){let z=q;z=Math.imul(z^(z>>>16),0x85ebca6b)>>>0;z=Math.imul(z^(z>>>13),0xc2b2ae35)>>>0;z=(z^(z>>>16))>>>0;u[i-s]=_$p[i]^(z&255)^((i*17)&255);}}const v=_$td.decode(u);_$c.set(k,v);return v;}
async function loadKernel(bytes){
const{ instance}= await WebAssembly.instantiate(bytes);
const _0x04310= instance.exports;
const _0x4a0a1= new TextEncoder();
const _0x8fe32= new TextDecoder();
return{
call(op, args={}){
_0x04310.reset();
const _0xd5bc3= _0x4a0a1.encode(JSON.stringify({ op,...args}));
const ptr= _0x04310.alloc(_0xd5bc3.length);
new Uint8Array(_0x04310.memory.buffer, ptr, _0xd5bc3.length).set(_0xd5bc3);
const _0x1b954= _0x04310.call(ptr, _0xd5bc3.length);
const start= Number(_0x1b954>> 32n);
const _0x616e5= Number(_0x1b954& 0xffffffffn);
return JSON.parse(_0x8fe32.decode(new Uint8Array(_0x04310.memory.buffer, start, _0x616e5)));
}
};
}
function makeScrubber(kernel, extra){
return(text)=>{
const _0xa7476= kernel.call(_$s431(0),{ text});
const _0xed207= extra? extra(_0xa7476.text):{ text: _0xa7476.text, removed: 0};
return{ text: _0xed207.text, removed: _0xa7476.map.length+ _0xed207.removed};
};
}
return{runCouncil:_$m0_d3b.runCouncil,estimateCouncil:_$m0_d3b.estimateCouncil,makeTransport:_$m1_752.makeTransport,parseModel:_$m1_752.parseModel,isLocalModel:_$m1_752.isLocalModel,ProviderError:_$m1_752.ProviderError,loadKernel,makeScrubber};})();
export const loadKernel=_$m2_431.loadKernel;
export const makeScrubber=_$m2_431.makeScrubber;
export const runCouncil=_$m2_431.runCouncil;
export const estimateCouncil=_$m2_431.estimateCouncil;
export const makeTransport=_$m2_431.makeTransport;
export const parseModel=_$m2_431.parseModel;
export const isLocalModel=_$m2_431.isLocalModel;
export const ProviderError=_$m2_431.ProviderError;
