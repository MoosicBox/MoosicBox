"""Rebuild the offline corpus and its assertion ledger from retained evidence."""
from pathlib import Path
import re, json, hashlib, shutil, tomllib
ROOT = Path(__file__).resolve().parent
SOURCES = [('upstream', 'bnjbvr/cargo-machete', 'main', '135555c9cd9f3b4723dee2766f0cacb99aac092b'), ('fork', 'BSteffaniak/cargo-machete', 'ignored-dirs', '87dd88aa744c5950fe155666ced33766ad62a338')]

def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)

def masked(s):
    # Keep offsets while ignoring Rust strings and comments when balancing syntax.
    return re.sub(r'r(#+)".*?"\1|"(?:\\.|[^"\\])*"|//[^\n]*|/\*.*?\*/', lambda m: ' ' * len(m[0]), s, flags=re.S)

def functions(s):
    mask = masked(s)
    for m in re.finditer(r'fn (test_\w+)\(', mask):
        if m[1] == 'test_one': continue
        start = mask.index('{', m.end()); depth = 1; end = start + 1
        while depth:
            depth += (mask[end] == '{') - (mask[end] == '}'); end += 1
        yield m[1], m.start(), end, s[m.start():end]

ledger = []; cases = []; variants = {}; comparisons = []

def declarations(base, manifest):
    doc = tomllib.loads((base / manifest).read_text())
    if 'package' not in doc: return []
    parent = (base / manifest).parent
    workspace = {}
    for p in [parent, *parent.parents]:
        if p == base.parent: break
        f = p / 'Cargo.toml'
        if f.exists():
            d = tomllib.loads(f.read_text())
            if 'workspace' in d: workspace = d['workspace']; break
    rows = []
    groups = [(None, doc)] + list(doc.get('target', {}).items())
    for target, group in groups:
        for table, kind in [('dependencies','NORMAL'), ('dev-dependencies','DEV'), ('build-dependencies','BUILD')]:
            for key, value in group.get(table, {}).items():
                inherited = isinstance(value,dict) and value.get('workspace',False)
                spec = workspace.get('dependencies',{}).get(key,{}) if inherited else value
                package = spec.get('package',key) if isinstance(spec,dict) else key
                rows.append(dict(manifest=manifest, package=doc['package']['name'], dependency=key, dependency_package=package, kind=kind, target=target, workspace=inherited))
    return rows

def add_case(label, source_id, name, fixture, manifest, unused, ignored=(), metadata=False, discovery=False):
    base = ROOT/'adapted'/label/fixture
    rows = declarations(base,manifest)
    inherited_ignored = set(ignored)
    for parent in [(base/manifest).parent, *(base/manifest).parent.parents]:
        if parent == base.parent: break
        config = parent/'clippier.toml'
        if config.exists(): inherited_ignored.update(tomllib.loads(config.read_text()).get('machete',{}).get('ignored',[]))
    for row in rows:
        row['status'] = 'IGNORED' if row['dependency'] in inherited_ignored else 'UNUSED' if row['dependency'] in unused else 'USED'
    assert set(unused) <= {r['dependency'] for r in rows}, (source_id,unused,rows)
    case_id = f'{label}:{name}:{len(cases)}'
    cases.append(dict(id=case_id, source_ids=[source_id], input=f'adapted/{label}/{fixture}', manifest=manifest, metadata=metadata, expected=rows, discovery=discovery))
    configured = {}
    original = ROOT/'original'/label/'integration-tests'/fixture
    if original.exists():
        for f in original.rglob('Cargo.toml'):
            d=tomllib.loads(f.read_text())
            for section in ('workspace','package'):
                configured.update(d.get(section,{}).get('metadata',{}).get('cargo-machete',{}).get('renamed',{}))
    cases[-1]['configured_names']=configured
    cases[-1]['ignored_used']=['rand_core'] if 'ignore_deps_' in source_id else []
    selected = original/manifest
    cases[-1]['used_ignore_scope'] = tomllib.loads(selected.read_text()).get('package',{}).get('metadata',{}).get('cargo-machete',{}).get('ignored',[]) if selected.exists() else []
    return case_id

for label, repo, branch, revision in SOURCES:
    original = ROOT/'original'/label
    variants[label] = sorted(p.name for p in (original/'integration-tests').iterdir() if p.is_dir())
    for fixture in variants[label]:
        src = original/'integration-tests'/fixture; dst=ROOT/'adapted'/label/fixture
        shutil.copytree(src,dst,dirs_exist_ok=True)
        for manifest in dst.rglob('Cargo.toml'):
            text=manifest.read_text(); doc=tomllib.loads(text); configs=[]
            for section in ('package','workspace'):
                legacy=doc.get(section,{}).get('metadata',{}).get('cargo-machete',{})
                if legacy: configs.append(legacy)
            if configs:
                lines=['[machete]']
                for key in ('ignored','ignored-dirs'):
                    values=sorted({v for c in configs for v in c.get(key,[])})
                    if values: lines.append(f'{key} = {json.dumps(values)}')
                write(manifest.parent/'clippier.toml','\n'.join(lines)+'\n')
            # Originals remain byte-exact. Only adapted inputs drop legacy settings.
            text=re.sub(r'(?ms)^\[(?:package|workspace)\.metadata\.cargo-machete(?:\.renamed)?\]\n.*?(?=^\[|\Z)','',text)
            # Cargo accepts leading ./ in member paths; normalize equivalent
            # spelling for the native glob boundary without changing membership.
            text=re.sub(r'(?m)^(members\s*=\s*\[.*?\])', lambda m: m[0].replace('"./','"'), text)
            manifest.write_text(text)
    for file in ['src/main.rs','src/search_unused.rs']:
        text=(original/file).read_text()
        for name,start,end,body in functions(text):
            source_id=f'{label}:{file}:{name}'
            assertions=[]; bm=masked(body)
            for a in re.finditer(r'\bassert(?:_eq|_ne)?!\s*\(',bm):
                pos=a.end(); depth=1
                while depth:
                    depth+=(bm[pos]=='(')-(bm[pos]==')');pos+=1
                assertions.append(dict(id=f'{source_id}:assertion-{len(assertions)+1}',line=text.count('\n',0,start+a.start())+1,text=body[a.start():pos]))
            entry=dict(id=source_id,file=file,name=name,line=text.count('\n',0,start)+1,assertions=assertions,executable_tests=[],case_ids=[])
            ledger.append(entry)
            if name=='test_regexp':
                # Each original positive/negative assertion becomes its own input.
                pat=r'assert!\(\s*(!?)test_one\(\s*"([^"\n]+)"\s*,\s*(r#+".*?"#+|"(?:\\.|[^"\\])*")\s*,?\s*\)\?\s*\)'
                matches=list(re.finditer(pat,body,re.S))
                assert len(matches)==len(assertions),(label,len(matches),len(assertions))
                for n,m in enumerate(matches):
                    literal=m[3]
                    content=literal[literal.index('"')+1:literal.rindex('"')] if literal.startswith('r') else json.loads(literal)
                    fixture=f'lexical-{n+1:02}'; base=ROOT/'adapted'/label/fixture
                    write(base/'Cargo.toml',f'[package]\nname = "lexical"\nversion = "0.1.0"\n[dependencies]\n{m[2]} = "1.0.0"\n')
                    write(base/'src/lib.rs',content+'\n')
                    cid=add_case(label,assertions[n]['id'],fixture,fixture,'Cargo.toml',[m[2]] if m[1] else [])
                    entry['case_ids'].append(cid)
                entry['executable_tests']=['lexical_roots_and_negative_boundaries']
            elif name=='test_ignore_dirs_parent_matching_flexibility':
                for n,m in enumerate(re.finditer(r'\("([^"]+)",\s*"([^"]+)",\s*(true|false)\)',body)):
                    pattern,path,expected=m.groups();fixture=f'parent-pattern-{n}';base=ROOT/'adapted'/label/fixture
                    write(base/'Cargo.toml',f'[package]\nname = "patterns"\nversion = "0.1.0"\n[workspace]\nmembers = [{json.dumps(path)}]\n[dependencies]\nmarker = "1.0.0"\n')
                    write(base/'src/lib.rs','pub fn main() {}\n')
                    # A nested package tests discovery, without relying on src-only scans.
                    write(base/path/'Cargo.toml','[package]\nname = "nested"\nversion = "0.1.0"\n[dependencies]\nmarker = "1.0.0"\n')
                    write(base/path/'src/lib.rs','marker::used();\n')
                    write(base/'clippier.toml',f'[machete]\nignored-dirs = [{json.dumps(pattern)}]\n')
                    cid=add_case(label,source_id,fixture,fixture,'Cargo.toml',['marker'])
                    cases[-1]['nested_manifest']=path+'/Cargo.toml'; cases[-1]['nested_excluded']=expected=='true'
                    entry['case_ids'].append(cid)
                entry['executable_tests']=['directory_patterns_match_ancestors_not_basename']
            elif name=='test_ignore_target':
                entry['executable_tests']=['target_directory_is_always_pruned']
                entry['case_ids']=[f'{label}:target:default',f'{label}:target:respect-ignore',f'{label}:target:no-ignore']
                entry['intentional_difference'] = 'Target discovery is nonconfigurable: always prune target, including no-ignore. A real root probe permits a successful empty dependency report.'
                for index, assertion in enumerate(assertions):
                    assertion['original_expected_discovery'] = 'nonempty' if index == 2 else 'empty'
                    assertion['native_expected_discovery'] = 'empty dependency rows; target packages absent; real root probe present'
                    assertion['classification'] = 'intentional-safety-difference' if index == 2 else 'equivalent-with-root-probe'
            elif name=='test_remove_dependencies':
                expected=re.search(r'assert_eq!\(\s*stripped_manifest,\s*r#"(.*?)"#',body,re.S)[1]
                write(ROOT/'removal'/label/'expected.toml',expected)
                entry['executable_tests']=['removes_every_dependency_section_without_reformatting','used_dependencies_cannot_be_forced_out']
                entry['case_ids']=[f'{label}:removal']
            else:
                paths=re.findall(r'"\./integration-tests/([^\"]+)"',body)
                if name=='test_workspace_from_relative_path': paths=['workspace-package/program/Cargo.toml']
                if name in ('test_ignore_dirs_workspace_nested_works','test_ignore_dirs_workspace_nested_deep_works'):
                    paths=[paths[0].rstrip('/')+'/inner/Cargo.toml']
                # check_analysis executes both metadata states for each callback.
                uses_helper='check_analysis(' in body
                expected_lists=[]
                if name=='test_unused_transitive': expected_lists=[[],[],['lib1']]
                elif name=='test_crate_renaming_works': expected_lists=[[],['xml-rs']]
                else:
                    mm=re.search(r'analysis\.unused(?:\.as_slice\(\))?,\s*&\[(.*?)\]',body,re.S)
                    if mm: expected_lists=[re.findall(r'"([^"]+)"',mm[1])]
                    elif 'analysis.unused.is_empty()' in body: expected_lists=[[]]
                    else:
                        mm=re.search(r'expected_unused\s*=\s*vec!\[(.*?)\]',body,re.S)
                        assert mm,(source_id,body)
                        expected_lists=[re.findall(r'"([^"]+)"',mm[1])]
                for i,path in enumerate(paths):
                    fixture,manifest=path.split('/',1)
                    expected=expected_lists[min(i,len(expected_lists)-1)]
                    modes=[False,True] if uses_helper else [name not in ('test_renamed_field_works','test_renamed_field_workspace_works','test_workspace_from_relative_path','test_ignore_dirs_workspace_nested_works','test_ignore_dirs_workspace_nested_deep_works')]
                    if name=='test_crate_renaming_works': modes=[i==0]
                    for metadata in modes:
                        ignored=['log','rand_core'] if name in ('test_ignore_deps_works','test_ignore_deps_workspace_works') else ['flagset'] if name=='test_renamed_field_works' else []
                        cid=add_case(label,source_id,name,fixture,manifest,expected,ignored,metadata,name in ('test_ignore_dirs_workspace_nested_works','test_ignore_dirs_workspace_nested_deep_works'))
                        cases[-1]['relative_input']=name=='test_workspace_from_relative_path'
                        entry['case_ids'].append(cid)
                entry['executable_tests']=['fixture_dependency_findings_are_exact']
            for assertion in assertions:
                assertion['executable_tests']=entry['executable_tests']; assertion['case_ids']=entry['case_ids']

for fixture in sorted(set(variants['upstream']) & set(variants['fork'])):
    a=ROOT/'original/upstream/integration-tests'/fixture;b=ROOT/'original/fork/integration-tests'/fixture
    files=sorted({str(p.relative_to(a)) for p in a.rglob('*') if p.is_file()}|{str(p.relative_to(b)) for p in b.rglob('*') if p.is_file()})
    changed=[f for f in files if not (a/f).exists() or not (b/f).exists() or (a/f).read_bytes()!=(b/f).read_bytes()]
    comparisons.append(dict(fixture=fixture,changed_files=changed))

sources=[]
for label,repo,branch,revision in SOURCES:
    evidence=[]
    for p in sorted((ROOT/'original'/label).rglob('*')):
        if p.is_file():
            rel=p.relative_to(ROOT/'original'/label).as_posix()
            evidence.append(dict(file=rel,url=f'https://github.com/{repo}/blob/{revision}/{rel}',sha256=hashlib.sha256(p.read_bytes()).hexdigest()))
    sources.append(dict(id=label,repository=f'https://github.com/{repo}',branch=branch,revision=revision,refreshed_head=revision,named_tests=sum(e['id'].startswith(label+':') for e in ledger),files=evidence))
assert [s['named_tests'] for s in sources]==[18,25]
assert len(set(variants['upstream'])|set(variants['fork']))==21
write(ROOT/'sources.json',json.dumps(dict(sources=sources,fixture_union=sorted(set(variants['upstream'])|set(variants['fork'])),shared_variations=comparisons,tests=ledger,cases=cases),indent=2)+'\n')
hashes={p.relative_to(ROOT).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(ROOT.rglob('*')) if p.is_file() and p.name!='sha256.json'}
write(ROOT/'sha256.json',json.dumps(hashes,indent=2)+'\n')
print(f'{len(ledger)} named tests, {sum(len(e["assertions"]) for e in ledger)} assertions, {len(cases)} executable cases, {len(hashes)} hashed files')
