#!/usr/bin/env python3
"""Resolve Linux x86_64 sample RIPs using captured maps and ELF symbols."""
import argparse,bisect,collections,json,struct,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('prefix',type=Path);p.add_argument('binary',type=Path);a=p.parse_args()
binary=a.binary.resolve();maps=[];biases={}
for line in Path(str(a.prefix)+'.maps').read_text().splitlines():
 fields=line.split(maxsplit=5)
 if len(fields)<6:continue
 span,perms,offset,device,inode,path=fields
 start,end=(int(v,16) for v in span.split('-'));offset=int(offset,16)
 maps.append((start,end,path,offset))
 if offset==0:biases.setdefault(path,start)
# The benchmark is ET_DYN with its first PT_LOAD at virtual address zero.
headers=subprocess.check_output(['readelf','-lW',str(binary)],text=True)
loads=[l.split() for l in headers.splitlines() if l.lstrip().startswith('LOAD ')]
assert int(loads[0][2],16)==0, 'first PT_LOAD must have vaddr0'
nm=subprocess.check_output(['nm','-n','-S','-C','--defined-only',str(binary)],text=True)
syms=[]
for line in nm.splitlines():
 fields=line.split(maxsplit=3)
 if len(fields)!=4:continue
 addr,size,kind,name=fields
 if kind not in 'tTwW':continue
 try:addr,size=int(addr,16),int(size,16)
 except ValueError:continue
 if size:syms.append((addr,size,name))
syms.sort();starts=[s[0] for s in syms]
raw=Path(str(a.prefix)+'.rips').read_bytes();assert len(raw)%8==0
modules=collections.Counter();functions=collections.Counter();instructions=collections.Counter();unknown=collections.Counter()
for (rip,) in struct.iter_unpack('<Q',raw):
 mapping=next((m for m in maps if m[0]<=rip<m[1]),None)
 if mapping is None:modules['unmapped']+=1;continue
 path=mapping[2];modules[path]+=1
 if path!=str(binary):continue
 relative=rip-biases[path];at=bisect.bisect_right(starts,relative)-1
 if at>=0 and relative<syms[at][0]+syms[at][1]:
  addr,size,name=syms[at];functions[name]+=1;instructions[(name,relative-addr,relative)]+=1
 else:unknown[relative]+=1
n=len(raw)//8
out={'samples':n,'binary':str(binary),'load_bias':hex(biases[str(binary)]),'modules':modules,
 'functions':[{'function':name,'samples':count,'percent_all':100*count/n} for name,count in functions.most_common()],
 'instructions':[{'function':name,'offset':hex(offset),'elf_address':hex(addr),'samples':count,'percent_all':100*count/n} for (name,offset,addr),count in instructions.most_common(80)],'unknown_binary_samples':sum(unknown.values()),'unknown_binary_offsets':{hex(k):v for k,v in unknown.most_common(20)}}
Path(str(a.prefix)+'.summary.json').write_text(json.dumps(out,indent=2)+'\n')
print('samples',n,'unknown',sum(unknown.values()))
for row in out['functions'][:15]:print(f"{row['percent_all']:5.1f}% {row['samples']:6d} {row['function']}")
