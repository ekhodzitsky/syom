from pathlib import Path
import datetime,hashlib,json,os,subprocess,time
root=Path('/home/ekhodzitsky/Projects/syom')
out=root/'target/task145-review/final-checks'
out.mkdir(exist_ok=False)
env=os.environ.copy();env['CARGO_NET_OFFLINE']='true';env['CARGO_BUILD_JOBS']='2'
commands=[['cargo','fmt','--check'],['python3','scripts/check-file-size.py'],['python3','scripts/check-changelog.py'],['cargo','clippy','--all-targets','--','-D','warnings'],['cargo','test','--workspace','--lib','--tests'],['cargo','test','--doc'],['python3','scripts/check-package.py'],['python3','-m','unittest','discover','-s','lab/consume/gigastt','-p','test_*.py']]

commands.append(['python3','lab/consume/gigastt/check_handoff.py','--consumer','target/task145/gigastt'])
commands.append(['python3','lab/consume/gigastt/http_reference.py','--canary','lab/release-audit/runs/task145-20261105/http-candidate5/canary/result.json','--independent','lab/release-audit/runs/task145-20261105/http-candidate5/independent/independent.json','--out','target/task145-review/replayed-reference.json'])
result={'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'scope':'Detailed TASK145 review: test-only FFT strengthening and qualification-tool refactor; production runtime unchanged. No hosted CI.','environment':{'CARGO_NET_OFFLINE':'true','CARGO_BUILD_JOBS':'2'},'commands':[]}
for i,cmd in enumerate(commands):
 start=time.monotonic();full=['taskset','-c','2-7']+cmd
 with (out/f'check-{i}.log').open('w') as log:r=subprocess.run(full,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
 row={'command':full,'exit_code':r.returncode,'seconds':round(time.monotonic()-start,3),'log':f'check-{i}.log'};result['commands'].append(row)
 (out/'checks.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(row),flush=True)
 if r.returncode:raise SystemExit(r.returncode)
crate=root/'target/package/syom-0.10.1.crate'
result['package']={'bytes':crate.stat().st_size,'sha256':hashlib.sha256(crate.read_bytes()).hexdigest(),'published':False}
result.update(finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),passed=True)
(out/'checks.json').write_text(json.dumps(result,indent=2)+'\n')
