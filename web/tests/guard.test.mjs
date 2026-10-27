import {test} from 'node:test';import assert from 'node:assert/strict';import {createEpoch,yen} from '../guard.mjs';
test('old response cannot replace a newer one',()=>{const e=createEpoch();const a=e.next();const b=e.next();assert.equal(e.isCurrent(a),false);assert.equal(e.isCurrent(b),true);});
test('scope change invalidates an in-flight response',()=>{const e=createEpoch();const a=e.next();e.next();assert.equal(e.isCurrent(a),false);});
test('integer money beyond Number precision is formatted exactly',()=>{assert.match(yen('9007199254740993'),/9,007,199,254,740,993/);});
