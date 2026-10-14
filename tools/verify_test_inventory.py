"""Compare nextest's full inventory with Cargo-built native libtest inventories."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[1]


def libtest_names(text):
    return {line[:-6] for line in text.splitlines() if line.endswith(': test')}


def compare_suite(cases,all_names,ignored_names):
    if set(cases)!=all_names or {name for name,case in cases.items() if case['ignored']}!=ignored_names:
        raise ValueError('Nextest and libtest inventories differ')
    if any(not case['ignored'] and case['filter-match']['status']!='matches' for case in cases.values()):
        raise ValueError('The CI profile excludes a non-ignored test')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,default=ROOT/'target/nextest/ci/inventory.json')
    args=parser.parse_args()
    built=subprocess.check_output(['cargo','test','--locked','--workspace','--all-features',
                                   '--no-run','--message-format=json'],cwd=ROOT)
    binaries={}
    for line in built.splitlines():
        item=json.loads(line)
        if item.get('reason')=='compiler-artifact' and item.get('profile',{}).get('test') and item.get('executable'):
            binaries[Path(item['executable']).resolve()]=item['target']['name']
    listed=json.loads(subprocess.check_output(['cargo','nextest','list','--locked','--workspace','--all-features',
                                              '--profile','ci','--message-format','json'],cwd=ROOT))
    suites={Path(suite['binary-path']).resolve():suite for suite in listed['rust-suites'].values()}
    if set(suites)-set(binaries): raise ValueError('Nextest listed an unqualified test executable')
    results=[]
    for binary,name in binaries.items():
        suite=suites.get(binary)
        cwd=Path(suite['cwd']) if suite else ROOT
        all_names=libtest_names(subprocess.check_output([str(binary),'--list','--format','terse'],cwd=cwd,text=True))
        ignored=libtest_names(subprocess.check_output([str(binary),'--list','--ignored','--format','terse'],cwd=cwd,text=True))
        if suite is None:
            if all_names: raise ValueError('Nextest omitted a nonempty test binary')
            cases={}
        else:
            if suite['status']!='listed': raise ValueError('A test binary was filtered out')
            cases=suite['testcases']
        compare_suite(cases,all_names,ignored)
        results.append({'binary':name,'tests':sorted(all_names),'ignored':sorted(ignored)})
    count=sum(len(result['tests']) for result in results)
    if not count or count!=listed['test-count']: raise ValueError('Incomplete test accounting')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps({'test_count':count,'binaries':results},indent=2)+'\n',encoding='utf-8')
    print(f'Inventory parity: {count} tests across {len(results)} native binaries; doctests run separately')


if __name__=='__main__': main()
