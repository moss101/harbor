"""Deterministic positive/negative contract regressions; no product PASS claims."""
import copy
import csv
import json
import shutil
import tempfile
from pathlib import Path
from contracts import ROOT, load, sha, canonical, validate_record, validate_event_append, validate_event_stream, validate_approval_binding, validate_effect_update, select_gates, evaluate_release

def run_regressions():
    from validate_dossier import rows, descriptor, check_package
    errors=[];passed=0
    def check(label,condition):
        nonlocal passed
        if condition:passed+=1
        else:errors.append('Regression failed: '+label)
    fixture=load(ROOT/'fixtures/contracts/records.json')
    records={c['id']:c['record'] for c in fixture['cases']}
    for c in fixture['cases']:
        check(c['id'],(not validate_record(c['schema'],c['record']))==c['expected_valid'])
    effect=records['effect-valid'];receipt=records['receipt-valid'];event=records['event-valid']
    bind=lambda e,r,**kw:validate_approval_binding(e,r,device_id='device-1',generation=1,now=kw.pop('now','2026-09-12T00:01:00Z'),**kw)
    check('approval-matches',not bind(effect,receipt))
    check('expired-approval-rejected',bool(bind(effect,receipt,now='2026-09-12T00:15:00Z')))
    check('terminated-run-rejected',bool(bind(effect,receipt,terminated=True)))
    check('changed-target-rejected',bool(bind(dict(effect,target=dict(effect['target'],identity='different')),receipt)))
    check('consumed-receipt-rejected',bool(bind(effect,dict(receipt,consumed_at='2026-09-12T00:00:30Z'))))
    check('stale-executor-rejected',bool(validate_event_append(event,current_generation=2)))
    check('first-event-valid',not validate_event_append(event,current_generation=1))
    following=dict(event,event_id='event-2',seq=1,prev_event_hash=sha(canonical(event)),event_type='run.transition',payload={'from_state':'CREATED','to_state':'PLANNING','reason':'start'})
    check('append-valid',not validate_event_append(following,event,current_generation=1))
    check('state-event-stale-system-lease-rejected',bool(validate_event_append(dict(following,actor='system',lease_generation=0),event,current_generation=1)))
    missing_lease=dict(following,actor='user');missing_lease.pop('lease_generation')
    check('state-event-missing-user-lease-rejected',bool(validate_record('run_event.schema.json',missing_lease)))
    check('replay-valid',not validate_event_stream([event,following],[1,1]))
    display=dict(following,event_id='display-3',seq=2,prev_event_hash=sha(canonical(following)),event_type='ui.message',replay_semantics='ignorable_display',payload={'message':'working'})
    wrong_state=dict(following,event_id='event-4',seq=3,prev_event_hash=sha(canonical(display)),payload={'from_state':'RUNNING','to_state':'COMPLETED','reason':'complete'})
    check('replay-discontinuous-state-rejected',bool(validate_event_stream([event,following,display,wrong_state],[1]*4)))
    repeated=dict(display,event_id=event['event_id'])
    check('replay-old-event-id-rejected',bool(validate_event_stream([event,following,repeated],[1]*3)))
    recreated=dict(event,event_id='event-3',seq=2,prev_event_hash=sha(canonical(following)))
    check('replay-second-creation-rejected',bool(validate_event_stream([event,following,recreated],[1]*3)))
    check('sequence-gap-rejected',bool(validate_event_append(dict(following,seq=3),event,current_generation=1)))
    check('hash-chain-rejected',bool(validate_event_append(dict(following,prev_event_hash='0'*64),event,current_generation=1)))
    p=dict(event,step_count_total=5)
    regressed=dict(following,prev_event_hash=sha(canonical(p)),step_count_total=4)
    check('counter-regression-rejected',bool(validate_event_append(regressed,p,current_generation=1)))
    dispatched=dict(effect,state='dispatched',attempt_id='attempt-1')
    check('effect-dispatch-valid',not validate_effect_update(effect,dispatched))
    check('effect-mutation-rejected',bool(validate_effect_update(effect,dict(dispatched,provider_key='new-key'))))
    check('uncertain-effect-not-redispatched',bool(validate_effect_update(dict(dispatched,state='outcome_unknown'),dispatched)))
    check('canonical-key-order',canonical({'z':1,'a':'ع'})==canonical({'a':'ع','z':1}))
    gates=rows('05_Acceptance_Matrix.csv');security=rows('09_Security_Test_Matrix.csv');registry=load(ROOT/'25_Feature_Registry.json')
    def select(d):return select_gates(d,gates,registry)
    baseline=descriptor()
    check('core-ga-selection-valid',not select(baseline)[1])
    check('unknown-feature-rejected',bool(select(dict(baseline,features=baseline['features']+['feature:unknown']))[1]))
    check('early-sync-rejected',bool(select(dict(baseline,features=baseline['features']+['feature:sync']))[1]))
    check('missing-feature-dependency-rejected',bool(select(dict(baseline,milestone='M4_OPTIONAL_SERVICES',features=baseline['features']+['feature:hf_gated']))[1]))
    check('core-cannot-disable-rag',bool(select(dict(baseline,features=['feature:hf_public']))[1]))
    for flag,f in registry['features'].items():
        for platform in f['platforms']:
            chosen=set(baseline['features'])|{flag};todo=[flag]
            while todo:
                current=todo.pop()
                for dep in registry['features'][current]['requires']:
                    if dep not in chosen:chosen.add(dep);todo.append(dep)
            d=descriptor(platform=platform,milestone='M4_OPTIONAL_SERVICES',features=sorted(chosen))
            states,issues=select(d)
            check('activation-'+platform+'-'+flag,not issues and all(states[g]=='REQUIRED' for g in f['activation_gates']))
    # End-to-end evidence integrity on a contract-only M0 candidate, never a product build.
    d=descriptor(milestone='M0_CONTRACT',features=[]);states,_=select(d);digest=sha(canonical(d));results=[]
    with tempfile.TemporaryDirectory(prefix='harbor-evidence-test-') as tmp:
        root=Path(tmp)
        for gid,state in states.items():
            if state!='REQUIRED':continue
            body={'gate_id':gid,'status':'PASS','release_descriptor_sha256':digest,'test_ids':['fixture.test'],'scenario_results':[]}
            p=root/(gid+'.json');p.write_text(json.dumps(body))
            r=dict(records['release-build-binding-valid'],gate_id=gid,release_descriptor_sha256=digest,commit_sha=d['commit_sha'],build_sha256=d['build_sha256'],target=d['target'],features=[],evidence=[{'path':p.name,'sha256':sha(p.read_bytes()),'test_ids':['fixture.test'],'media_type':'application/json'}])
            results.append(r)
        evaluate=lambda rs:evaluate_release(d,rs,gates,security,registry,root)
        check('bound-m0-evidence-valid',evaluate(results)['qualified'])
        check('missing-gate-evidence-rejected',not evaluate(results[:-1])['qualified'])
        bad=copy.deepcopy(results);bad[0]['build_sha256']='c'*64
        check('wrong-build-rejected',not evaluate(bad)['qualified'])
        bad=copy.deepcopy(results);bad[0]['evidence'][0]['path']='missing.json'
        check('missing-evidence-file-rejected',not evaluate(bad)['qualified'])
        p=root/results[0]['evidence'][0]['path'];p.write_text('[]')
        bad=copy.deepcopy(results);bad[0]['evidence'][0]['sha256']=sha(p.read_bytes())
        check('nonobject-evidence-report-rejected',not evaluate(bad)['qualified'])
        p.write_text('tampered')
        check('tampered-evidence-rejected',not evaluate(results)['qualified'])
    # Mutation checks ensure package validation catches the actual review regressions.
    with tempfile.TemporaryDirectory(prefix='harbor-ledger-test-') as tmp:
        root=Path(tmp)/'package'
        shutil.copytree(ROOT,root,ignore=shutil.ignore_patterns('assets','reviews','outputs','*.docx','__pycache__','target','.git','.github','build','.dart_tool','.gradle','ephemeral','.idea','Pods'))
        def mutate(filename,key,ident,field,value,needle):
            p=root/filename;original=p.read_bytes();records=rows(filename,root)
            next(r for r in records if r[key]==ident)[field]=value
            with p.open('w',newline='') as f:
                w=csv.DictWriter(f,fieldnames=list(records[0]));w.writeheader();w.writerows(records)
            report=check_package(root,check_documents=False)
            check(needle,any(needle in issue for issue in report['issues']))
            p.write_bytes(original)
        mutate('04_Implementation_Backlog.csv','Task ID','HBR-080','Milestone','M3_GA_CORE','Later dependency')
        mutate('05_Acceptance_Matrix.csv','ID','ACC-009','Feature Applicability','feature:hf_public','Protected approvals must be core')
        mutate('07_Screen_Inventory.csv','ID','UX-016','Required States','loading;ready','Missing run detail states')
    return {'passed':passed,'failed':len(errors),'errors':errors}

if __name__=='__main__':
    result=run_regressions();print(json.dumps(result,indent=2));raise SystemExit(bool(result['failed']))
