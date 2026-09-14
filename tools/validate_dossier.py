#!/usr/bin/env python3
"""Validate Harbor specification structure or evaluate supplied product evidence.

Requires Python 3.9+ and requirements-validation.txt. No network or product calls.
"""
import argparse
import csv
import importlib.metadata
import json
import sys
from collections import Counter
from pathlib import Path
from zipfile import ZipFile
import xml.etree.ElementTree as ET

from jsonschema import Draft202012Validator
from contracts import ROOT, load, sha, select_gates, evaluate_release

VERSION = 'harbor-dossier-validator/1.0.0'
DERIVED = {'18_Reconciled_Release_Matrix.csv','19_Structural_Validation.json','20_Readiness_Validation_Report.md','24_PACKAGE_MANIFEST.json'}

def rows(name,root=ROOT):
    with (root/name).open(newline='',encoding='utf-8-sig') as f:return list(csv.DictReader(f))
def refs(s):return [x for x in s.split(';') if x]
def package_paths(root=ROOT):
    # Seal exactly the REPOSITORY content, not machine-local state: prefer
    # git-tracked files so a fresh checkout (CI) reproduces the manifest.
    # Machine-generated files that happen to be gitignored
    # (GeneratedPluginRegistrant, local.properties, Flutter ephemeral
    # state, gradle wrappers) previously leaked into the walk and made the
    # seal irreproducible outside this machine.
    try:
        import subprocess
        tracked = subprocess.run(['git','ls-files','-z'],cwd=root,capture_output=True,check=True).stdout.split(b'\0')
        paths=[]
        for raw in tracked:
            if not raw:continue
            p=root/raw.decode('utf-8')
            if p.is_file() and p.name!='24_PACKAGE_MANIFEST.json':paths.append(p)
        return sorted(paths)
    except (subprocess.CalledProcessError, FileNotFoundError):
        pass  # not a git checkout: fall back to the walk below
    return sorted(p for p in root.rglob('*') if p.is_file() and not any(part.startswith('.') or part=='__pycache__' or part=='target' or part=='build' for part in p.relative_to(root).parts) and p.relative_to(root).parts[0] not in {'reviews','outputs','evidence'} and p.name!='24_PACKAGE_MANIFEST.json')
def inventory(paths,root=ROOT):
    return [{'path':str(p.relative_to(root)),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in paths]
def input_digest(root=ROOT):
    entries=inventory([p for p in package_paths(root) if p.name not in DERIVED],root)
    return sha(json.dumps(entries,sort_keys=True,separators=(',',':')).encode()),entries

def descriptor(root=ROOT,platform='Windows',milestone='M3_GA_CORE',features=None):
    return {'schema':'harbor.release/v1','release_id':'spec-validation-fixture','milestone':milestone,'target':{'platform':platform,'architecture':{'iOS':'arm64','Android':'arm64-v8a','macOS':'arm64','Windows':'x64'}[platform],'os_version':'fixture-only','device_class':'fixture-only','qualification_profile':'fixture-only'},'features':features if features is not None else ['feature:hf_public','feature:rag','feature:ocr_ar'],'commit_sha':'a'*40,'build_sha256':'b'*64,'gate_catalog_sha256':sha((root/'05_Acceptance_Matrix.csv').read_bytes()),'feature_registry_sha256':sha((root/'25_Feature_Registry.json').read_bytes()),'created_at':'2026-09-12T00:00:00Z'}

def check_package(root=ROOT,check_documents=True):
    errors=[];notes=[]
    sources={'tasks':('04_Implementation_Backlog.csv','Task ID'),'gates':('05_Acceptance_Matrix.csv','ID'),'screens':('07_Screen_Inventory.csv','ID'),'security':('09_Security_Test_Matrix.csv','ID')}
    data={};lookup={}
    for name,(path,key) in sources.items():
        data[name]=rows(path,root);lookup[name]={r[key]:r for r in data[name]}
        if len(data[name])!=len(lookup[name]):errors.append('Duplicate IDs in '+path)
        if any(None in r or any(v is None for v in r.values()) for r in data[name]):errors.append('Malformed CSV row in '+path)
    T,G,S,Q=[lookup[k] for k in ['tasks','gates','screens','security']]
    registry=load(root/'25_Feature_Registry.json');milestones=registry['milestones']; flags=registry['features']
    mi=lambda r:milestones.index(r['Milestone'])
    mappings={'tasks':{'Dependencies':'tasks','Acceptance IDs':'gates','Security IDs':'security','Screen IDs':'screens'},'gates':{'Implementing Tasks':'tasks','Security IDs':'security','Screen IDs':'screens'},'screens':{'Task IDs':'tasks','Recovery Routes':'screens'},'security':{'Gate IDs':'gates'}}
    for name,fields in mappings.items():
        for r in data[name]:
            rid=r[sources[name][1]]
            for field,target in fields.items():
                for ref in refs(r[field]):
                    if ref not in lookup[target]:errors.append(f'Unknown {field} reference {rid}/{ref}')
            feature=r.get('Feature Applicability','core')
            if feature!='core' and feature not in flags:errors.append('Unknown feature on '+rid)
            if 'Milestone' in r and r['Milestone'] not in milestones:errors.append('Unknown milestone on '+rid)
            if 'Platforms' in r and r['Platforms']!='All' and not set(refs(r['Platforms'])) <= set(registry['platforms']):errors.append('Unknown platform selector on '+rid)
    if errors:return {'validator_version':VERSION,'issues':errors}
    for tid,t in T.items():
        for dep in refs(t['Dependencies']):
            if mi(T[dep])>mi(t):errors.append(f'Later dependency: {tid} -> {dep}')
        if not t['Acceptance IDs']:errors.append('Task without acceptance path: '+tid)
        actual={gid for gid,g in G.items() if tid in refs(g['Implementing Tasks'])}
        if set(refs(t['Acceptance IDs']))!=actual:errors.append('Stale task gate backlinks: '+tid)
    visiting=set();done=set()
    def visit(tid):
        if tid in visiting:errors.append('Dependency cycle: '+tid);return
        if tid in done:return
        visiting.add(tid)
        for d in refs(T[tid]['Dependencies']):visit(d)
        visiting.remove(tid);done.add(tid)
    for tid in T:visit(tid)
    for gid,g in G.items():
        if not refs(g['Implementing Tasks']):errors.append('Gate has no implementing task: '+gid)
        for tid in refs(g['Implementing Tasks']):
            if mi(T[tid])>mi(g):errors.append(f'Gate precedes task: {gid} -> {tid}')
            f=T[tid]['Feature Applicability']
            allowed={'core',g['Feature Applicability']}
            if mi(g)>=3:allowed.update(registry['required_at_core_ga'])
            if g['Feature Applicability'] in flags:allowed.update(flags[g['Feature Applicability']]['requires'])
            if f not in allowed:errors.append(f'Gate requires unrelated optional task: {gid} -> {tid} ({f})')
    if G['ACC-009']['Feature Applicability']!='core':errors.append('Protected approvals must be core')
    if T['HBR-100']['Feature Applicability']!='core':errors.append('Egress broker must be core')
    if T['HBR-103']['Feature Applicability']!='feature:connectors':errors.append('Connector task has wrong feature')
    for sid,s in S.items():
        if set(refs(s['Task IDs']))=={'HBR-160'}:errors.append('Screen lacks implementation task: '+sid)
        for tid in refs(s['Task IDs']):
            if mi(T[tid])>mi(s):errors.append('Screen precedes task: '+sid+'/'+tid)
        if s['Required Languages']!='en;ar' or s['Required Themes']!='light;dark':errors.append('Missing screen language/theme coverage: '+sid)
    expected={'ACC-047':{'UX-008','UX-009','UX-010','UX-047'},'ACC-052':{'UX-008','UX-010'},'ACC-061':{'UX-008','UX-009','UX-010'},'ACC-063':{'UX-009','UX-010','UX-016','UX-017'}}
    for gid,expected_ids in expected.items():
        if not expected_ids <= set(refs(G[gid]['Screen IDs'])):errors.append('Incorrect semantic screen map: '+gid)
    if not {'running','paused','waiting_approval','cancelling','cancelled','outcome_unknown'} <= set(refs(S['UX-016']['Required States'])):errors.append('Missing run detail states')
    if not {'formula_unverified','recalculating','conflict','saving'} <= set(refs(S['UX-009']['Required States'])):errors.append('Missing spreadsheet states')
    for sid,s in Q.items():
        owning={gid for gid,g in G.items() if sid in refs(g['Security IDs'])}
        if not owning or not s['Test ID']:errors.append('Uncovered security scenario: '+sid)
        if owning != set(refs(s['Gate IDs'])):errors.append('Stale security backlinks: '+sid)
        if not any(G[g]['Feature Applicability'] in {'core',s['Feature Applicability']} for g in owning):errors.append('Scenario only mapped to unrelated optional gates: '+sid)
    projection={'Gate ID':'ID','Milestone':'Milestone','Blocking':'Blocking','Applicability':'Feature Applicability','Platforms':'Platforms','Owner':'Owner','Tasks':'Implementing Tasks','Security IDs':'Security IDs','Screen IDs':'Screen IDs','Evidence':'Evidence Required'}
    projected=[{a:g[b] for a,b in projection.items()} for g in data['gates']]
    if rows('18_Reconciled_Release_Matrix.csv',root)!=projected:errors.append('Release projection differs from acceptance authority')
    # Activation coverage is checked here; the regression suite exercises every
    # feature on each supported platform, including dependency and early-activation failures.
    for name,feature in flags.items():
        if feature['default_enabled'] is not False:errors.append('Optional feature must default off: '+name)
        if not feature['activation_gates']:errors.append('Feature has no activation gates: '+name)
        for gid in feature['activation_gates']:
            if gid not in G or G[gid]['Feature Applicability']!=name:errors.append('Wrong activation gate owner: '+name+'/'+gid)
            elif G[gid]['Milestone']!=feature['earliest_milestone']:errors.append('Activation gate milestone mismatch: '+name+'/'+gid)
        if not set(feature['requires'])<=set(flags):errors.append('Unknown feature dependency: '+name)
    counts={}
    for platform in registry['platforms']:
        states,issues=select_gates(descriptor(root,platform),data['gates'],registry,root)
        errors.extend(issues);counts[platform]=sum(v=='REQUIRED' for v in states.values())
    for schema in sorted((root/'schemas').glob('*.schema.json')):
        try:Draft202012Validator.check_schema(load(schema))
        except Exception as exc:errors.append(schema.name+': '+str(exc))
    tokens=load(root/'06_Design_Tokens.json')
    if tokens['layout'].get('workCanvasMinAppliesAtViewport')!=1024:errors.append('Desktop canvas minimum lacks viewport boundary')
    if tokens['layout'].get('desktopRail')!=tokens['layout'].get('desktopRailExpanded'):errors.append('Conflicting rail tokens')
    def lum(s):
        parts=[int(s[i:i+2],16)/255 for i in (1,3,5)]
        return sum(w*(v/12.92 if v<=0.04045 else ((v+0.055)/1.055)**2.4) for w,v in zip([.2126,.7152,.0722],parts))
    def contrast(a,b):
        lo,hi=sorted([lum(a),lum(b)]);return (hi+.05)/(lo+.05)
    pairs=[]
    for theme,roles in tokens['semanticRoles'].items():
        for role,pair in roles.items():
            if isinstance(pair,dict):pairs.append({'theme':theme,'pair':role,'ratio':contrast(pair['text'],pair['fill'])})
    for theme,palette in tokens['color'].items():
        for item in tokens['accessibility']['allowedTextSurfacePairs']:
            for surface in item['surfaces']:pairs.append({'theme':theme,'pair':item['text']+'/'+surface,'ratio':contrast(palette[item['text']],palette[surface])})
    for p in pairs:
        if p['ratio']<4.5:errors.append('Failed contrast: '+p['theme']+'/'+p['pair'])
    profiles=load(root/'26_Qualification_Profiles.json')
    for fixture in profiles['fixture_sources']:
        if sha((root/fixture['path']).read_bytes())!=fixture['sha256']:errors.append('Qualification fixture source hash mismatch')
    formula=load(root/'22_Formula_Coverage.json')
    functions=[f for group in formula['target_groups'] for f in group['functions']]
    for f in functions:
        if f['status']=='PASS' and not formula['qualification_results']:errors.append('Formula PASS lacks bound qualification results')
    if check_documents:
        ns={'w':'http://schemas.openxmlformats.org/wordprocessingml/2006/main'}
        for p in root.glob('*.docx'):
            with ZipFile(p) as z:
                text=' '.join(x.text or '' for x in ET.fromstring(z.read('word/document.xml')).findall('.//w:t',ns))
            for stale in ['114-task','63 release gates','47 security scenarios with no gaps','harbor.artifact_batch/v2','harbor.effect/v2','harbor.run_event/v2','harbor.model/v2','five machine-readable schemas','If prose conflicts with a schema or the acceptance matrix, the schema/acceptance matrix wins.']:
                if stale in text:errors.append('Stale Word authority text: '+p.name+' / '+stale)
            if 'Revision 3' not in text:errors.append('Word authority missing corrected revision: '+p.name)
    digest,inputs=input_digest(root)
    return {'validator_version':VERSION,'status':'PASS' if not errors else 'FAIL','scope':'Specification structure and contract regressions only; no product release evidence','input_manifest_sha256':digest,'input_files':inputs,'counts':{k:len(v) for k,v in data.items()},'schema_count':len(list((root/'schemas').glob('*.schema.json'))),'feature_count':len(flags),'task_priorities':dict(Counter(t['Priority'] for t in data['tasks'])),'dependency_acyclic':not any('cycle' in x for x in errors),'later_dependency_count':sum('Later dependency' in x for x in errors),'core_ga_required_gate_counts_by_platform':counts,'contrast_checks':pairs,'formula_target_count':len(functions),'formula_qualified_pass_count':sum(f['status']=='PASS' for f in functions),'unbound_product_qualification_fields':[k for k,v in profiles['production_bindings'].items() if v is None],'issues':errors}

def write_reports(report,root=ROOT):
    (root/'19_Structural_Validation.json').write_text(json.dumps(report,indent=2)+'\n')
    counts=report['counts']
    text='# Harbor dossier validation report\n\n'
    text+=f"Specification validation: **{report['status']}**. Validator `{VERSION}`.\n\n"
    text+=f"{counts['tasks']} tasks, {counts['gates']} gates, {counts['screens']} screens, {counts['security']} security scenarios, {report['schema_count']} schemas and {report['feature_count']} explicit features.\n\n"
    text+='Checks cover ID and backlink integrity, milestone ordering, feature activation, security coverage, screen semantics, schema validity, negative contract fixtures, color pairs, document revision and package integrity.\n\n'
    text+='Required core GA gates by platform for public Hub, RAG and Arabic OCR: '+', '.join(f'{p}: {n}' for p,n in report['core_ga_required_gate_counts_by_platform'].items())+'. Counts describe gate definitions; each architecture/device qualification still needs evidence.\n\n'
    if report.get('contract_regressions'):text+=f"Contract regression cases: {report['contract_regressions']['passed']} passed, {report['contract_regressions']['failed']} failed.\n\n"
    text+='Product readiness remains **BLOCKED**. No product gate is marked PASS by this report. Formula qualification remains 0 of '+str(report['formula_target_count'])+'; model, engine, adapter, fixture, evaluation and device bindings require implementation evidence. Performance thresholds must be approved and measured.\n\n'
    text+='Input manifest SHA-256: `'+report['input_manifest_sha256']+'`. File 19 records every input digest. The package manifest excludes itself and includes this report.\n\n'
    text+='Reproduce: install `requirements-validation.txt`, then run `python3 tools/validate_dossier.py`. Regenerate derived reports only after authority edits with `python3 tools/validate_dossier.py --write`.\n\n'
    text+='Issues: '+('None.\n' if not report['issues'] else '\n\n'+'\n'.join('- '+x for x in report['issues'])+'\n')
    (root/'20_Readiness_Validation_Report.md').write_text(text)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write',action='store_true');parser.add_argument('--skip-documents',action='store_true',help='Development only; cannot seal package')
    parser.add_argument('--release',type=Path);parser.add_argument('--results',type=Path);parser.add_argument('--evidence-root',type=Path)
    args=parser.parse_args()
    if args.release:
        if not args.results or not args.evidence_root:parser.error('--release requires --results and --evidence-root')
        report=evaluate_release(load(args.release),load(args.results),rows('05_Acceptance_Matrix.csv'),rows('09_Security_Test_Matrix.csv'),load(ROOT/'25_Feature_Registry.json'),args.evidence_root)
        print(json.dumps(report,indent=2));return 0 if report['qualified'] else 1
    report=check_package(check_documents=not args.skip_documents)
    from test_contracts import run_regressions
    tests=run_regressions();report['contract_regressions']=tests
    report['issues'].extend(tests['errors'])
    report['status']='PASS' if not report['issues'] else 'FAIL'
    if args.write:
        if args.skip_documents:parser.error('Cannot seal without document checks')
        if report['issues']:print(json.dumps(report['issues'],indent=2));return 1
        write_reports(report)
        manifest={'package':'Harbor Implementation Authority','revision':'3','date':'2026-09-12','authority_status':'Corrected specification; product qualification remains evidence-blocked','validator_version':VERSION,'input_manifest_sha256':report['input_manifest_sha256'],'file_count_excluding_manifest':len(package_paths()),'files':inventory(package_paths())}
        (ROOT/'24_PACKAGE_MANIFEST.json').write_text(json.dumps(manifest,indent=2)+'\n')
    elif not args.skip_documents:
        manifest=load(ROOT/'24_PACKAGE_MANIFEST.json')
        if manifest.get('files')!=inventory(package_paths()):report['issues'].append('Package manifest differs; regenerate only after review')
        saved=load(ROOT/'19_Structural_Validation.json')
        if saved.get('input_manifest_sha256')!=report['input_manifest_sha256']:report['issues'].append('Saved validation does not match current inputs')
    compact={k:v for k,v in report.items() if k not in ['input_files','contrast_checks']}
    compact['status']='PASS' if not report['issues'] else 'FAIL'
    print(json.dumps(compact,indent=2));return 0 if not report['issues'] else 1

if __name__=='__main__':sys.exit(main())
