#!/usr/bin/env python3
"""Local, non-destructive probes of CO-1 documentation validation.
Uses synthetic Markdown and invokes only the supplied Python validation function.
No application/runtime/security target, GitHub write, or external request.
"""
from pathlib import Path
import importlib.util
import json
import sys

ROOT = Path(__file__).resolve().parents[1]
PKG = ROOT
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('co1_check_docs', PKG/'tools/check_docs.py')
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
OUT = ROOT/'evidence'/'revision-1.1'/'probes'
OUT.mkdir(parents=True, exist_ok=True)


def base(kind='reference'):
    prefix, count = checker.SECTIONS[kind]
    fm = f'''---
schema: corelink-ownership/1.1
document: {kind}
package: corelink-example
manifest: crates/corelink-example/Cargo.toml
source_commit: {'a'*40}
profile: S
state: draft
evidence_set: example-source-set
---

# Example

'''
    nav = ' '.join(f'[Section {i}](#{prefix}{i:02d})' for i in range(1,count+1))+'\n\n'
    body = ''.join(f'<a id="{prefix}{i:02d}"></a>\n## {prefix.upper()}{i:02d} — Example\n\nExample section.\n\n' for i in range(1,count+1))
    return fm+nav+body


results = []
def probe(name, text, kind, expectation, classification):
    path = OUT/f'{name}.md'
    path.write_text(text, encoding='utf-8')
    report = checker.validate(text,kind,'S',path=path,root=ROOT)
    expected_rejection = expectation == 'reject'
    rejected = bool(report['errors'])
    result = {'case':name, 'expected':expectation,'classification':classification,
              'observed':report['verdict'],'errors':report['errors'],
              'meets_expectation':rejected==expected_rejection,
              'fixture':str(path.relative_to(ROOT))}
    results.append(result)
    return result

r = base()
probe('control_valid_structure',r,'reference','pass','positive_control_not_semantic_approval')
legacy = r.replace('schema: corelink-ownership/1.1','schema: corelink-ownership/1')
probe('legacy_schema_rejected_without_historical_mode',legacy,'reference','reject','current_artifact_contract')
probe('control_missing_section',r.replace('<a id="r08"></a>\n## R08 — Example',''),
      'reference','reject','negative_control')
probe('control_placeholder',r+'\n{{MISSING}}\n','reference','reject','negative_control')
probe('control_long_code',r+'\n```text\n'+('line\n'*26)+'```\n',
      'reference','reject','negative_control')
# Same section exists only as code, not as a rendered section or anchor.
probe('section_in_code_counts_as_section',
      r.replace('<a id="r08"></a>\n## R08 — Example',
                '```markdown\n<a id="r08"></a>\n## R08 — Example\n```'),
      'reference','reject','implemented_section_check_false_acceptance')
# Relationship exceeds both bounds. An inner HTML anchor is not a new record.
long_lines = ''.join('- '+('detail '*9)+'\n' for _ in range(18))
card = '<a id="rel-001"></a>\n### REL-001 — Example\n'+long_lines+'[Index](#b03)\n\n'
b = base('blast_radius')
probe('control_long_relation',b.replace('<a id="b04"></a>',card+'<a id="b04"></a>'),
      'blast_radius','reject','negative_control')
probe('inner_anchor_stops_relation_count',
      b.replace('<a id="b04"></a>',card.replace('### REL-001 — Example\n',
               '### REL-001 — Example\n<a id="rel-001-detail"></a>\n')+'<a id="b04"></a>'),
      'blast_radius','reject','implemented_record_bound_false_acceptance')
# Document metadata is present but wrong. These are explicitly partial checks,
# therefore record the gap without claiming full YAML validation was promised.
probe('document_kind_disagrees_with_cli',r.replace('document: reference','document: maintenance'),
      'reference','reject','unimplemented_contract_consistency_check')
probe('empty_evidence_set',r.replace('evidence_set: example-source-set','evidence_set:'),
      'reference','reject','unimplemented_contract_consistency_check')
# Two ordinary summary paragraphs are each <80 words, combined >120 words.
summary = ('summary '*70)+'\n\n'+('summary '*70)+'\n'
probe('entry_summary_exceeds_120_words',r.replace('Example section.',summary,1),
      'reference','reject','unimplemented_entry_summary_check')
# A new record exists but there is no index entry for it.
short_card='<a id="rel-001"></a>\n### REL-001 — Example\n**Contract:** example.\n[Index](#b03)\n\n'
probe('relation_missing_from_index',b.replace('<a id="b04"></a>',short_card+'<a id="b04"></a>'),
      'blast_radius','reject','unimplemented_record_navigation_check')
# Skill metadata conforms, but an overlong description violates the CO-1 ceiling.
skill = '---\nname: own-example\ndescription: '+('d'*601)+'\nmetadata:\n  package: "example"\n---\n\n# Example\n\n'
skill += ' '.join(f'[Section {i}](#s{i:02d})' for i in range(1,8))+'\n\n'
skill += ''.join(f'<a id="s{i:02d}"></a>\n## S{i:02d} — Example\n\n' for i in range(1,8))
skill_dir = OUT/'own-example';skill_dir.mkdir(exist_ok=True)
skill_path = skill_dir/'SKILL.md';skill_path.write_text(skill)
skill_result = checker.validate(skill,'skill','S',path=skill_path,root=ROOT)
results.append({'case':'skill_description_exceeds_project_limit','expected':'reject',
 'classification':'unimplemented_skill_description_check','observed':skill_result['verdict'],
 'errors':skill_result['errors'],'meets_expectation':bool(skill_result['errors']),
 'fixture':str(skill_path.relative_to(ROOT))})
output = {'scope':'synthetic documentation fixtures; no CoreLink runtime tests',
          'template_mode':False, 'results':results,
          'summary': {'cases':len(results),'met':sum(x['meets_expectation'] for x in results),
                      'false_acceptances':sum(x['expected']=='reject' and not x['meets_expectation'] for x in results)}}
(ROOT/'evidence'/'revision-1.1'/'adversarial-results.json').write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps(output,indent=2))
# This is a contract probe: nonzero means at least one expected check is missing.
raise SystemExit(0 if all(item['meets_expectation'] for item in results) else 1)
