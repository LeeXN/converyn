#!/usr/bin/env python3
"""Validate generated schemas and captured real HTTP requests/responses.
Development dependency: jsonschema>=4 (Draft 2020-12). No credentials are captured.
"""
from pathlib import Path
import json,re,sys,subprocess
from jsonschema import Draft202012Validator
root=Path(__file__).resolve().parents[1]
subprocess.run([sys.executable,str(root/'tools/generate_api.py'),'--check'],check=True)
doc=json.loads((root/'docs/schema/openapi.json').read_text())
for name,schema in doc['components']['schemas'].items():Draft202012Validator.check_schema(schema)
source=(root/'crates/runtime/src/controller/server/mod.rs').read_text()
routes=set(re.findall(r'\.route\(\s*"([^"]+)"',source))
assert routes==set(doc['paths']),(routes-set(doc['paths']),set(doc['paths'])-routes)
ids=[op["operationId"] for methods in doc["paths"].values() for op in methods.values()]
assert len(ids)==len(set(ids)),"duplicate OpenAPI operation IDs"
def validate(value,schema):Draft202012Validator({'components':doc['components'],**schema}).validate(value)
cases=0;covered=set()
if len(sys.argv)>1:
 for line in Path(sys.argv[1]).read_text().splitlines():
  case=json.loads(line);path=case['path'].split('?')[0]
  match=next((p for p in doc['paths'] if re.fullmatch(re.sub(r'\{[^}]+\}',r'[^/]+',p),path)),None)
  assert match,case['path'];operation=doc['paths'][match][case['method']]
  schema=operation['responses'][str(case['status'])]['content']['application/json']['schema']
  validate(case['response'],schema)
  if 200<=case['status']<300 and 'requestBody' in operation:
   validate(case['request'],operation['requestBody']['content']['application/json']['schema'])
  cases+=1;covered.add((case['method'],match,case['status']))
print(json.dumps({'passed':True,'routes':len(routes),'schemas':len(doc['components']['schemas']),'real_http_cases':cases,'covered_response_shapes':len(covered)}))
