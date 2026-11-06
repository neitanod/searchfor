#!/usr/bin/env bash
set -euo pipefail
# One Python program: --self-test and the measurements decide with the same
# `decide` and `verdict` (plan r1, Codex #6).
[[ $# == 1 && ${1} == --self-test ]] ||
  [[ $# == 1 && -n ${SEARCHFOR_BIN:-} && -n ${SEARCHFOR_BASE_BIN:-} && -n ${SEARCHFOR_BENCH_REPORT:-} ]]
python3 -I - "$1" <<'PY'
import json, os, pathlib, statistics, subprocess, sys, tempfile, time
def decide(medians,limit,additive=0):
    # medians: one (s4, reference) pair of medians per series. An additive gate
    # bounds the median excess in seconds; any other gate the median ratio.
    ratio=statistics.median(a/b for a,b in medians)
    excess=statistics.median(a-b for a,b in medians)
    return ratio,excess,(excess<=additive if additive else ratio<=limit)
def verdict(gates): return 0 if gates and all(g['ok'] for g in gates) else 1
if sys.argv[1]=='--self-test':
    for limit in (1.02,1.03,1.05,1.1):
        assert decide([(limit,1)]*3,limit)[2]
        assert not decide([(limit+0.0001,1)]*3,limit)[2]
        # The median of three series decides, not the worst or the best one.
        assert decide([(limit+0.5,1),(limit,1),(1,1)],limit)[2]
        assert not decide([(limit+0.5,1),(limit+0.001,1),(1,1)],limit)[2]
    # Additive gates, with values exact in binary: the ratio does not count.
    for additive in (0.25,0.0625):
        assert decide([(1+additive,1)]*3,0,additive)[2]
        assert not decide([(1+additive+0.001,1)]*3,0,additive)[2]
        assert decide([(0.25+additive,0.25)]*3,0,additive)[2]
    assert verdict([{'ok':True},{'ok':True}])==0
    assert verdict([{'ok':True},{'ok':False}])==1 and verdict([])==1
    print('M6 cost gates: PASS')
    raise SystemExit(0)
root=pathlib.Path(sys.argv[1]).resolve()
s4=os.environ['SEARCHFOR_BIN']; base=os.environ['SEARCHFOR_BASE_BIN']; rg=os.environ.get('SEARCHFOR_RG','/usr/bin/rg')
assert subprocess.check_output([rg,'--version']).startswith(b'ripgrep 14.1.1')
meta=json.loads((root/'metadata.json').read_text()); report={'raw':[], 'gates':[], 'uptime':subprocess.check_output(['uptime']).decode()}
assert 0<meta['matching_files']/meta['files']<.05
threads=int(os.environ.get('SEARCHFOR_BENCH_THREADS',os.cpu_count() or 1))
def sample(tool,args,pattern,path):
    command=[tool,'--no-config','-j',str(threads),*args,pattern,str(path)]
    start=time.perf_counter()
    result=subprocess.run(command,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,env=env)
    elapsed=time.perf_counter()-start
    assert result.returncode==(0 if pattern=='M6MARK' else 1),(command,result.returncode,result.stderr)
    report['raw'].append({'command':command,'seconds':elapsed})
    return elapsed
def paired(label,a,b,pattern,path,limit,additive=0):
    medians=[]
    for series in range(3):
        samples=[[],[]]
        for warm in range(3):
            sample(a[0],a[1],pattern,path);sample(b[0],b[1],pattern,path)
        for pair in range(40):
            for side in ((0,1) if pair%2==0 else (1,0)):
                tool,args=(a,b)[side];samples[side].append(sample(tool,args,pattern,path))
        ta,tb=map(statistics.median,samples);medians.append((ta,tb))
    ratio,excess,ok=decide(medians,limit,additive)
    report['gates'].append({'name':label,'medians':medians,'ratio':ratio,'excess':excess,'limit':additive or limit,'ok':ok})
    print(label,ratio,excess,'PASS' if ok else 'FAIL',flush=True)
with tempfile.TemporaryDirectory() as iso:
    env=os.environ.copy();env.pop('RIPGREP_CONFIG_PATH',None);env.pop('SEARCHFOR_CONFIG_PATH',None)
    for var,leaf in [('SEARCHFOR_FORMATS_DIR','formats'),('SEARCHFOR_GRAMMARS_DIR','grammars')]:
        p=pathlib.Path(iso)/leaf;p.mkdir();env[var]=str(p)
    env['SEARCHFOR_GRAMMAR_INDEX']='file://'+str(pathlib.Path(iso)/'missing.json')
    raw=['--no-docs','--no-trail']; flat=['--no-docs','--flat']
    # A fast time earns nothing from a Plain fallback: the matched sample of mixed/ is every
    # Match of a real M6 Format, and no unclaimed name is a Telegram or WhatsApp export.
    def formats(path,pattern):
        out=subprocess.run([s4,'--no-config','--no-docs','--json',pattern,str(path)],
                           stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,check=True).stdout
        rows=[json.loads(l) for l in out.splitlines() if l.strip()]
        return [r['data'].get('format') for r in rows if r['type']=='match']
    mixed=formats(root/'mixed','M6MARK')
    # rg's files with the mark: a hidden one (`.env.m6`) is no more searched by s4 than by rg.
    found=subprocess.run([rg,'--no-config','-l','M6MARK',str(root/'mixed')],stdout=subprocess.PIPE,
                         env=env,check=True).stdout.count(b'\n')
    assert 0<found<=len(mixed) and all(mixed),('Plain fallback in mixed/',found,mixed)
    report['mixed_formats']=sorted(set(mixed))
    unclaimed=set(formats(root/'unclaimed'/'0','ordinary'))
    assert not unclaimed&{'telegram','telegram-html','whatsapp'},unclaimed
    report['unclaimed_formats']=sorted(map(str,unclaimed))
    paired('mixed-no-match-no-trail',(s4,raw),(rg,[]),'ABSENT_M6',root/'mixed',1.1)
    paired('mixed-no-match-trail',(s4,flat),(rg,[]),'ABSENT_M6',root/'mixed',1.1)
    paired('mixed-sparse',(s4,flat),(rg,[]),'M6MARK',root/'mixed',0,
           meta['largest_matching_bytes']/50_000_000+meta['matching_bytes']/(50_000_000*threads)+.05)
    paired('unclaimed-detection',(s4,flat),(base,flat),'ABSENT_M6',root/'unclaimed',1.02)
    # Existing M4 text corpus must already exist under root/m4/text.
    m4=root/'m4/text';assert m4.is_dir()
    paired('m4-no-trail-rg',(s4,raw),(rg,[]),'ABSENT_M6',m4,1.05)
    paired('m4-trail-rg',(s4,['--no-docs']),(rg,[]),'ABSENT_M6',m4,1.05)
    paired('m4-default-no-docs',(s4,[]),(s4,['--no-docs']),'ABSENT_M6',m4,1.05)
    for flags in (raw,flat):
        paired('m4-vs-m5b-'+','.join(flags),(s4,flags),(base,flags),'ABSENT_M6',m4,1.03)
pathlib.Path(os.environ['SEARCHFOR_BENCH_REPORT']).write_text(json.dumps(report,indent=2))
raise SystemExit(verdict(report['gates']))
PY
