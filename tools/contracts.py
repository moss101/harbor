"""Harbor dossier reference validation. No tool dispatch or product execution."""
import hashlib
import json
import re
import unicodedata
from datetime import datetime, timedelta
from pathlib import Path, PurePosixPath

from jsonschema import Draft202012Validator, FormatChecker

ROOT = Path(__file__).resolve().parents[1]
TRANSITIONS = {
    'CREATED': {'PLANNING', 'CANCELLING', 'FAILED'},
    'PLANNING': {'RUNNING', 'PAUSED', 'CANCELLING', 'FAILED'},
    'RUNNING': {'WAITING_APPROVAL', 'PAUSED', 'CANCELLING', 'COMPLETED', 'FAILED'},
    'WAITING_APPROVAL': {'RUNNING', 'PAUSED', 'CANCELLING', 'FAILED'},
    'PAUSED': {'RUNNING', 'CANCELLING'},
    'CANCELLING': {'CANCELLED', 'PAUSED'},
    'COMPLETED': set(), 'FAILED': set(), 'CANCELLED': set(),
}
COUNTERS = ('active_compute_ms_total', 'step_count_total', 'tool_count_total', 'context_tokens_total')

def load(path):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError('Duplicate JSON key: ' + key)
            result[key] = value
        return result
    return json.loads(Path(path).read_text(), object_pairs_hook=pairs,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))

def sha(data):
    return hashlib.sha256(data).hexdigest()

def canonical(value):
    def check(v):
        if v is None or isinstance(v, bool):
            return
        if isinstance(v, int):
            if abs(v) > 9007199254740991:
                raise ValueError('Integer outside exact signed 53-bit range')
        elif isinstance(v, str):
            v.encode('utf-8', errors='strict')
        elif isinstance(v, list):
            for child in v: check(child)
        elif isinstance(v, dict):
            for key, child in v.items():
                if not isinstance(key, str): raise ValueError('Non-string JSON key')
                check(key); check(child)
        else:
            raise ValueError('Canonical arguments prohibit floating-point and non-JSON values')
    check(value)
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False).encode('utf-8')

def at(value):
    t = datetime.fromisoformat(value.replace('Z', '+00:00'))
    if t.utcoffset() is None: raise ValueError('Timezone required')
    return t

def validate_record(name, record, root=ROOT):
    schema = load(root / 'schemas' / name)
    Draft202012Validator.check_schema(schema)
    errors = [f'{e.json_path}: {e.message}' for e in Draft202012Validator(schema, format_checker=FormatChecker()).iter_errors(record)]
    if errors: return errors
    if name == 'effect_intent.schema.json':
        try:
            if sha(canonical(record['canonical_args'])) != record['canonical_args_hash']:
                errors.append('canonical_args_hash does not match canonical arguments')
        except (ValueError, UnicodeError) as exc: errors.append(str(exc))
        if at(record['updated_at']) < at(record['created_at']): errors.append('Effect timestamp regresses')
    if name == 'approval_receipt.schema.json':
        issued, expires = at(record['issued_at']), at(record['expires_at'])
        if not issued < expires <= issued + timedelta(minutes=15): errors.append('Receipt lifetime must be positive and at most 15 minutes')
        if record['consumed_at'] is not None and not issued <= at(record['consumed_at']) < expires:
            errors.append('Consumption outside receipt validity')
    if name == 'artifact_batch.schema.json':
        ids = [op['op_id'] for op in record['operations']]
        if len(ids) != len(set(ids)): errors.append('Duplicate operation ID')
    if name == 'model_package.schema.json' and record['reference_type'] == 'installed_package':
        names = set()
        reserved = {'CON', 'PRN', 'AUX', 'NUL'} | {f'{p}{i}' for p in ('COM', 'LPT') for i in range(1, 10)}
        for f in record['files']:
            name = f['path']
            key = unicodedata.normalize('NFC', name).casefold()
            if key in names: errors.append('Normalized/case-colliding package paths')
            names.add(key)
            for part in name.split('/'):
                if part != part.rstrip(' .') or part.split('.')[0].upper() in reserved:
                    errors.append('Reserved or ambiguous package path')
    if name == 'run_event.schema.json' and record['event_type'] == 'run.transition':
        p = record['payload']
        if p['to_state'] not in TRANSITIONS[p['from_state']]: errors.append('Illegal run transition')
        if p['from_state'] == 'CANCELLING' and p['to_state'] == 'PAUSED' and p['reason'] != 'cancellation_unacknowledged':
            errors.append('Cancellation pause requires cancellation_unacknowledged')
    return errors

def validate_event_append(event, previous=None, current_generation=None, root=ROOT):
    errors = validate_record('run_event.schema.json', event, root)
    if errors: return errors
    authoritative = event['replay_semantics'] != 'ignorable_display'
    if event['actor'] == 'executor' or authoritative:
        if current_generation is None or event['lease_generation'] != current_generation:
            errors.append('Missing or stale lease generation')
    if previous:
        if event['event_type'] == 'run.created': errors.append('Run cannot be created twice')
        if event['run_id'] != previous['run_id']: errors.append('Cross-run append')
        if event['event_id'] == previous['event_id']: errors.append('Duplicate event identity')
        if event['seq'] != previous['seq'] + 1: errors.append('Noncontiguous event sequence')
        if event['prev_event_hash'] != sha(canonical(previous)): errors.append('Broken event hash chain')
        for key in COUNTERS:
            if event[key] < previous[key]: errors.append('Counter regressed: ' + key)
    elif event['seq'] != 0 or event['prev_event_hash'] is not None or event['event_type'] != 'run.created':
        errors.append('First event must create the run at sequence zero')
    return errors

def validate_event_stream(events, generations, root=ROOT):
    """Fold a stream against per-event generations obtained from trusted lease state."""
    if not events or len(events) != len(generations):
        return ['A nonempty stream needs one trusted generation per event']
    errors=[]; previous=None; state=None; seen=set()
    for event,generation in zip(events,generations):
        issues=validate_event_append(event,previous,current_generation=generation,root=root)
        if issues:return errors+issues
        if event['event_id'] in seen:errors.append('Duplicate event identity in stream')
        seen.add(event['event_id'])
        if event['event_type']=='run.created':state='CREATED'
        elif event['event_type']=='run.transition':
            if event['payload']['from_state']!=state:errors.append('Transition does not match replayed state')
            state=event['payload']['to_state']
        previous=event
    return errors

def validate_approval_binding(effect, receipt, *, device_id, generation, now, terminated=False, batch=None, root=ROOT):
    errors = validate_record('effect_intent.schema.json', effect, root) + validate_record('approval_receipt.schema.json', receipt, root)
    if errors: return errors
    if effect['approval_receipt_id'] != receipt['receipt_id']: errors.append('Receipt ID mismatch')
    for k in ['run_id', 'effect_id', 'executor_generation', 'effect_class', 'canonical_args_hash', 'target', 'policy_version']:
        if effect[k] != receipt[k]: errors.append('Approval binding mismatch: ' + k)
    if receipt['device_id'] != device_id or receipt['executor_generation'] != generation: errors.append('Device or generation mismatch')
    if receipt['decision'] != 'approved' or receipt['consumed_at'] is not None: errors.append('Receipt denied or consumed')
    if terminated or not at(receipt['issued_at']) <= at(now) < at(receipt['expires_at']): errors.append('Receipt expired, future or run terminated')
    if effect['effect_class'] == 'file_write':
        if batch is None: errors.append('Artifact approval requires batch')
        else:
            errors += validate_record('artifact_batch.schema.json', batch, root)
            for k in ['batch_id', 'base_content_hash', 'proposed_output_hash']:
                if receipt[k] != batch[k]: errors.append('Artifact approval mismatch: ' + k)
            if batch['approval_receipt_id'] != receipt['receipt_id']: errors.append('Batch receipt mismatch')
    return errors

def validate_effect_update(previous, updated, root=ROOT):
    errors = validate_record('effect_intent.schema.json', previous, root) + validate_record('effect_intent.schema.json', updated, root)
    if errors: return errors
    mutable = {'state', 'attempt_id', 'provider_result_ref', 'updated_at'}
    if {k:v for k,v in previous.items() if k not in mutable} != {k:v for k,v in updated.items() if k not in mutable}:
        errors.append('Immutable effect intent changed')
    allowed = {'prepared': {'dispatched', 'aborted'}, 'dispatched': {'committed', 'outcome_unknown', 'aborted'}, 'outcome_unknown': {'committed', 'aborted'}, 'committed': set(), 'aborted': set()}
    if previous['state'] != updated['state'] and updated['state'] not in allowed[previous['state']]: errors.append('Illegal effect transition')
    if at(updated['updated_at']) < at(previous['updated_at']): errors.append('Effect update timestamp regresses')
    return errors

def platform_applies(selector, platform):
    return selector == 'All' or platform in selector.split(';')

def select_gates(descriptor, gates, registry, root=ROOT):
    errors = validate_record('release_descriptor.schema.json', descriptor, root)
    if errors: return {}, errors
    features = set(descriptor['features']); mi = registry['milestones'].index(descriptor['milestone']); platform = descriptor['target']['platform']
    for path,key in [('05_Acceptance_Matrix.csv','gate_catalog_sha256'), ('25_Feature_Registry.json','feature_registry_sha256')]:
        if sha((root/path).read_bytes()) != descriptor[key]: errors.append('Stale descriptor input digest: '+path)
    for feature in features:
        spec = registry['features'].get(feature)
        if spec is None: errors.append('Unknown feature: ' + feature); continue
        if registry['milestones'].index(spec['earliest_milestone']) > mi: errors.append('Feature enabled before activation milestone: '+feature)
        if platform not in spec['platforms']: errors.append('Feature unsupported on platform: '+feature)
        if not set(spec['requires']) <= features: errors.append('Missing feature dependencies: '+feature)
    if mi >= 3 and not set(registry['required_at_core_ga']) <= features: errors.append('Missing mandatory core GA capability')
    states={}
    for g in gates:
        feature=g['Feature Applicability']
        if not platform_applies(g['Platforms'],platform): status='N/A_PLATFORM'
        elif feature!='core' and feature not in features: status='N/A_DISABLED'
        elif registry['milestones'].index(g['Milestone'])>mi: status='N/A_MILESTONE'
        else: status='REQUIRED' if g['Blocking']=='Yes' else 'ADVISORY'
        states[g['ID']]=status
    for feature in features & registry['features'].keys():
        required=registry['features'][feature]['activation_gates']
        if not required or any(states.get(g)!='REQUIRED' for g in required): errors.append('Missing required activation gates: '+feature)
    return states,errors

def evidence_path(root, relative):
    p=(root/relative).resolve()
    if not p.is_relative_to(root.resolve()): raise ValueError('Evidence path escapes evidence root')
    return p

def evaluate_release(descriptor, results, gates, security, registry, evidence_root, root=ROOT):
    states,errors=select_gates(descriptor,gates,registry,root)
    if errors:return {'qualified':False,'errors':errors,'gate_states':states}
    by_id={}; digest=sha(canonical(descriptor))
    for r in results:
        record_errors=validate_record('release_gate.schema.json',r,root)
        if record_errors: errors.extend(record_errors);continue
        if r['gate_id'] in by_id: errors.append('Duplicate result: '+r['gate_id'])
        by_id[r['gate_id']]=r
        if r['gate_id'] not in states: errors.append('Unknown gate result: '+r['gate_id'])
        if r['release_descriptor_sha256']!=digest: errors.append('Descriptor binding mismatch: '+r['gate_id'])
        for field in ['commit_sha','build_sha256','target']:
            if r[field]!=descriptor[field]: errors.append('Result binding mismatch: '+field)
        if set(r['features'])!=set(descriptor['features']): errors.append('Result feature set mismatch')
    for gid,state in states.items():
        result=by_id.get(gid)
        if state!='REQUIRED':
            if result and result['status']!=state: errors.append('Incorrect exclusion result: '+gid)
            continue
        if not result or result['status']!='PASS':errors.append('Missing or non-PASS required gate: '+gid);continue
        reports=[]
        for item in result['evidence']:
            try:
                p=evidence_path(Path(evidence_root),item['path'])
                if sha(p.read_bytes())!=item['sha256']:raise ValueError('Evidence hash mismatch')
                if item['media_type']=='application/json':
                    report=load(p)
                    if not isinstance(report,dict):raise ValueError('Evidence report must be a JSON object')
                    if report.get('gate_id')!=gid or report.get('status')!='PASS' or report.get('release_descriptor_sha256')!=digest:raise ValueError('Evidence report binding/status mismatch')
                    if not isinstance(report.get('test_ids'),list) or not all(isinstance(x,str) for x in report['test_ids']):raise ValueError('Evidence test IDs must be a string array')
                    if not isinstance(report.get('scenario_results',[]),list) or not all(isinstance(x,dict) for x in report.get('scenario_results',[])):raise ValueError('Evidence scenarios must be an object array')
                    if not set(item['test_ids']) <= set(report.get('test_ids',[])):raise ValueError('Evidence test ID mismatch')
                    reports.append(report)
            except (OSError,ValueError,TypeError) as exc:errors.append(gid+': '+str(exc))
        if not reports:errors.append('Missing bound JSON test report: '+gid)
        g=next(x for x in gates if x['ID']==gid)
        for scenario in security:
            applies=scenario['Feature Applicability']=='core' or scenario['Feature Applicability'] in descriptor['features']
            if not applies or not platform_applies(scenario['Platforms'],descriptor['target']['platform']):continue
            if scenario['ID'] not in g['Security IDs'].split(';'):continue
            if not any(any(x.get('id')==scenario['ID'] and x.get('test_id')==scenario['Test ID'] and x.get('status')=='PASS' for x in report.get('scenario_results',[])) for report in reports):
                errors.append('Missing scenario PASS: '+gid+'/'+scenario['ID'])
        qualification_fields={
            'ACC-051':['formula_engine_source_revision','formula_engine_sha256','formula_adapter_revision','binary_fixture_bundle_sha256','qualified_device_manifest_sha256'],
            'ACC-052':['binary_fixture_bundle_sha256','qualified_device_manifest_sha256'],
            'ACC-054':['model_package_sha256','qualified_device_manifest_sha256'],
            'ACC-056':['model_package_sha256','evaluation_corpus_sha256','qualified_device_manifest_sha256'],
            'ACC-063':['model_package_sha256','formula_engine_source_revision','formula_engine_sha256','formula_adapter_revision','binary_fixture_bundle_sha256','qualified_device_manifest_sha256'],
            'ACC-081':['model_package_sha256','formula_engine_source_revision','formula_engine_sha256','formula_adapter_revision','binary_fixture_bundle_sha256','qualified_device_manifest_sha256'],
        }
        if gid in qualification_fields:
            profiles=load(root/'26_Qualification_Profiles.json')
            fields=qualification_fields[gid]
            if any(profiles['production_bindings'].get(k) is None for k in fields):errors.append('Unbound qualification profile blocks '+gid)
            elif not any(report.get('qualification_profile_sha256')==sha((root/'26_Qualification_Profiles.json').read_bytes()) and all(report.get('qualification_identity',{}).get(k)==profiles['production_bindings'][k] for k in fields) for report in reports):
                errors.append('Qualification identity mismatch: '+gid)
    return {'qualified':not errors,'errors':errors,'gate_states':states,'required_gate_count':sum(s=='REQUIRED' for s in states.values())}
