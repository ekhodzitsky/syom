import subprocess,pathlib,json,time,shutil
root=pathlib.Path('/home/ekhodzitsky/Projects/syom')
steps=[('fmt',['cargo','fmt','--check']),('size',['python3','scripts/check-file-size.py']),('changelog',['python3','scripts/check-changelog.py']),('clippy',['cargo','clippy','--all-targets','--','-D','warnings']),('tests',['cargo','test','--workspace','--lib','--tests']),('docs',['cargo','test','--doc']),('package',['python3','scripts/check-package.py']),('msrv',['cargo','+1.89.0','test','--workspace','--lib','--tests']),('consumer',['python3','lab/consume/run.py']),('fuzz',['cargo','run','--release','--manifest-path','lab/fuzz/Cargo.toml','--bin','syom_fuzz_state','--','--seconds','30','--seed','1124']),('peer-build',['cargo','build','--release','--manifest-path','lab/baseline/peer_speed/Cargo.toml']),('stack-build',['cargo','build','--release','--manifest-path','lab/footprint/Cargo.toml','--bin','stack'])]
steps += [('stack-'+mode,['lab/footprint/target/release/stack',mode,'128']) for mode in ['enc-lc','enc-he2','enc-mc']]
rows=[]
for name,cmd in steps:
 log=pathlib.Path('/tmp/syom-lc2-final-'+name+'.log');t=time.monotonic()
 with log.open('w') as f: code=subprocess.run(cmd,cwd=root,stdout=f,stderr=subprocess.STDOUT).returncode
 row={'step':name,'command':cmd,'returncode':code,'wall_seconds':time.monotonic()-t,'log':log.name};rows.append(row);print(json.dumps(row),flush=True)
 pathlib.Path('/tmp/syom-lc2-final-validation.json').write_text(json.dumps(rows,indent=2)+'\n')
 if code:raise SystemExit(code)
shutil.copy2(root/'lab/baseline/peer_speed/target/release/syom-peer-speed','/tmp/syom-lc2-reviewed-peer')
print('ALL FINAL VALIDATION PASS',flush=True)
