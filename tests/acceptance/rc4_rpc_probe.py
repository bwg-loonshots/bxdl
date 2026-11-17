"""Bounded read-only RC4 RPC integration cases on an authenticated test node.

The caller authenticates the fixed runtime and owns the fresh isolated network.
This module has no process/network/import side effects and cannot submit a tx.
It does not certify signed Type4 execution, KZG provider fault propagation,
Native recipe execution, or simulation isolation for successful authorizations.
"""
import hashlib
import json
import re
import time

import support as h

KZG_ADDRESS = '0x' + '0' * 39 + 'a'
VALID_KZG_INPUT = ('0x'
    '01ad7666ef9d8f53b5adf54f029b13b6f171b1d0bd346a2ede315d3e243484ef'
    '73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000'
    '0000000000000000000000000000000000000000000000000000000000000000'
    '93efc82d2017e9c57834a1246463e64774e56183bb247c8fc9dd98c56817e878'
    'd97b05f5c8d900acf1fbbbca6f14655692c51ff81dd71dab71cefecd79e8274b'
    '4b7ba36a0f40e2dc086bc4061c7f63249877db23297212991fd63e07b7ebc348')
INVALID_KZG_INPUT = (VALID_KZG_INPUT[:2 + 144 * 2]
    + '9779b8337f00de6aeac881256198bd2db2fe95bc3127ad9e6440d9e4d1e785b455f55fcfe80a3434dc40f8e6df85be88')
KZG_OUTPUT = ('0x0000000000000000000000000000000000000000000000000000000000001000'
    '73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001')
HOST_ADDRESSES = tuple('0x' + format(n, '040x') for n in (0x105, 0x106, 0x107))
READ_METHODS = {'eth_chainId', 'eth_getBalance', 'eth_getTransactionCount', 'eth_getCode',
                'eth_call', 'eth_estimateGas'}
MAX_REQUESTS = 52
MAX_SECONDS = 120


def _digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def cases(sender):
    h.require(isinstance(sender, str) and re.fullmatch(r'0x[0-9a-f]{40}', sender) is not None,
              'RC4_RPC_SENDER_INVALID')
    rows = []
    def add(label, method, target, data, result=None, error=None, extra=None):
        request = {'from':sender, 'to':target, 'data':data, 'value':'0x0', 'gas':'0x1e8480'}
        request.update(extra or {})
        rows.append({'id':label, 'method':method, 'params':[request, 'latest'],
                     'expected':{'error':error} if error else {'result':result}})
    error = {'code':-32000, 'message':'Top-level precompile execution failed: ' + KZG_ADDRESS}
    add('kzg-valid-call', 'eth_call', KZG_ADDRESS, VALID_KZG_INPUT, result=KZG_OUTPUT)
    add('kzg-valid-estimate', 'eth_estimateGas', KZG_ADDRESS, VALID_KZG_INPUT, result=hex(73628))
    add('kzg-invalid-proof-call', 'eth_call', KZG_ADDRESS, INVALID_KZG_INPUT, error=error)
    add('kzg-invalid-proof-estimate', 'eth_estimateGas', KZG_ADDRESS, INVALID_KZG_INPUT, error=error)
    add('kzg-invalid-length-estimate', 'eth_estimateGas', KZG_ADDRESS, VALID_KZG_INPUT[:-2], error=error)
    # Valid private-host READ bytes must remain ordinary public EVM calldata.
    host_input = '0x06010103' + '00' * 32 + sender[2:]
    for target in HOST_ADDRESSES:
        for label, data in [('empty', '0x'), ('host-read', host_input)]:
            gas = 21000 + 10 * sum(4 if byte else 1 for byte in bytes.fromhex(data[2:]))
            for method in ('eth_call', 'eth_estimateGas'):
                add('public-host-' + target[-4:] + '-' + label + '-' + method,
                    method, target, data, result='0x' if method == 'eth_call' else hex(gas))
    for method in ('eth_call', 'eth_estimateGas'):
        add('sha256-control-' + method, method, '0x' + format(2, '040x'), '0x616263',
            result='0x' + hashlib.sha256(b'abc').hexdigest() if method == 'eth_call' else hex(21120))
        add('type4-empty-authorizations-' + method, method, sender, '0x',
            extra={'type':'0x4', 'chainId':hex(11578), 'nonce':'0x0',
                   'maxFeePerGas':'0x0', 'maxPriorityFeePerGas':'0x0',
                   'accessList':[], 'authorizationList':[]},
            error={'code':-32602, 'message':'Invalid params: authorizationList must be a nonempty array'})
    h.require(len(rows) == 21, 'RC4_RPC_CASE_COUNT')
    return rows


def run_probe(call, sender, clock=time.monotonic, record=None):
    """Preserve partial counts/cases on failure; never turn failure into PASS."""
    report = {} if record is None else record
    h.require(isinstance(report, dict) and not report, 'RC4_RPC_FRESH_RECORD_REQUIRED')
    report.update({'outcome':'INCOMPLETE', 'scope':'READ_ONLY_FIXED_RC4_RPC_CASES',
                   'cases':[], 'requestCount':0, 'automaticRetries':0, 'submittedTransactions':0})
    start = clock()
    try:
        completed = _run_probe(call, sender, clock, report)
        report.update(completed)
        return report
    except BaseException as error:
        report['reason'] = str(error) if isinstance(error, h.Failure) else 'RC4_RPC_' + type(error).__name__.upper()
        report['durationSeconds'] = clock() - start
        raise


def _run_probe(call, sender, clock, record):
    """Execute exactly 52 bounded reads or fail without retry/submission.

    call must enforce its own per-request transport timeout and response limit.
    This budget is checked before and after every call, not an async interrupt.
    """
    rows = cases(sender)
    start = clock()
    count = 0
    observations = record['cases']
    def invoke(method, params):
        nonlocal count
        h.require(method in READ_METHODS and count < MAX_REQUESTS, 'RC4_RPC_REQUEST_LIMIT')
        h.require(clock() - start <= MAX_SECONDS, 'RC4_RPC_DEADLINE')
        count += 1
        record['requestCount'] = count
        record['lastRequest'] = {'number':count, 'method':method,
                                 'sha256':_digest({'method':method, 'params':params})}
        record.pop('lastResponseSha256', None)
        value = call(method, params)
        record['lastResponseSha256'] = _digest(value)
        h.require(clock() - start <= MAX_SECONDS, 'RC4_RPC_DEADLINE')
        h.require(isinstance(value, dict) and value.get('jsonrpc') == '2.0'
                  and type(value.get('id')) is int and value['id'] == 1
                  and set(value) in ({'jsonrpc','id','result'}, {'jsonrpc','id','error'}),
                  'RC4_RPC_ENVELOPE')
        if 'error' in value:
            error = value['error']
            h.require(isinstance(error, dict) and set(error) == {'code', 'message'}
                      and type(error['code']) is int and isinstance(error['message'], str),
                      'RC4_RPC_ERROR_CONTRACT')
        return {key:value[key] for key in ('result','error') if key in value}
    def read(method, params):
        response = invoke(method, params)
        h.require('result' in response, 'RC4_RPC_STATE_UNAVAILABLE')
        return response['result']
    def snapshot():
        state = {}
        for address in (sender, *HOST_ADDRESSES, KZG_ADDRESS):
            account = {}
            for method in ('eth_getBalance', 'eth_getTransactionCount', 'eth_getCode'):
                value = read(method, [address, 'latest'])
                pattern = r'0x(?:[0-9a-f]{2})*' if method == 'eth_getCode' else r'0x(?:0|[1-9a-f][0-9a-f]*)'
                h.require(isinstance(value, str) and len(value) <= 65536
                          and re.fullmatch(pattern, value) is not None, 'RC4_RPC_STATE_INVALID')
                account[method] = value
            state[address] = account
        return state
    h.require(read('eth_chainId', []) == hex(11578), 'RC4_RPC_CHAIN_ID')
    before = snapshot()
    for row in rows:
        observed = invoke(row['method'], row['params'])
        matched = observed == row['expected']
        observations.append({'case':row['id'], 'outcome':'PASS' if matched else 'FAIL',
                             'requestSha256':_digest({'method':row['method'],'params':row['params']}),
                             'responseSha256':_digest(observed)})
        h.require(matched, 'RC4_RPC_EXPECTATION_' + row['id'].upper().replace('-', '_'))
    after = snapshot()
    h.require(before == after, 'RC4_RPC_ACCOUNT_STATE_CHANGED')
    h.require(count == MAX_REQUESTS, 'RC4_RPC_REQUEST_COUNT')
    return {'outcome':'PASS', 'scope':'READ_ONLY_FIXED_RC4_RPC_CASES', 'cases':observations,
            'caseCount':len(rows), 'requestCount':count, 'automaticRetries':0, 'submittedTransactions':0,
            'observedAccountStateUnchanged':True, 'stateSha256':_digest(before),
            'durationSeconds':clock()-start, 'signedType4Execution':'NOT_RUN',
            'successfulAuthorizationSimulationIsolation':'NOT_RUN', 'kzgProviderFault':'NOT_RUN',
            'nativeRecipeExecution':'NOT_RUN', 'consoleCatalogProjection':'NOT_RUN'}
