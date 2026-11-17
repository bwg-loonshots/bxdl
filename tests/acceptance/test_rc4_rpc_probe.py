import copy
import unittest

import rc4_rpc_probe as probe
import support as h

SENDER = '0x' + '12' * 20


class Rc4RpcProbeTests(unittest.TestCase):
    def source(self):
        cases = probe.cases(SENDER)
        self.calls = []
        def call(method, params):
            self.calls.append((method, params))
            if method == 'eth_chainId':result = {'result':hex(11578)}
            elif method == 'eth_getCode':result = {'result':'0x'}
            elif method in ('eth_getBalance','eth_getTransactionCount'):result = {'result':'0x0'}
            else:
                row = next(r for r in cases if r['method'] == method and r['params'] == params)
                result = copy.deepcopy(row['expected'])
            return {'jsonrpc':'2.0','id':1,**result}
        return call

    def test_fixed_vectors_and_finite_read_only_sequence(self):
        raw = bytes.fromhex(probe.VALID_KZG_INPUT[2:])
        self.assertEqual(len(raw),192)
        self.assertEqual(21000 + sum(4 if b == 0 else 16 for b in raw) + 50000,73628)
        self.assertEqual(raw[:144],bytes.fromhex(probe.INVALID_KZG_INPUT[2:])[:144])
        result = probe.run_probe(self.source(),SENDER)
        self.assertEqual(result['requestCount'],52)
        self.assertEqual(result['caseCount'],21)
        self.assertEqual(result['submittedTransactions'],0)
        self.assertEqual(result['signedType4Execution'],'NOT_RUN')
        self.assertTrue(all(method in probe.READ_METHODS for method,_ in self.calls))

    def test_wrong_expected_error_result_and_envelopes_fail_without_retry(self):
        for mutation in ('code','result','missing','id','both'):
            source = self.source()
            def call(method,params):
                value = source(method,params)
                if method == 'eth_call':
                    if mutation == 'code':return {'jsonrpc':'2.0','id':1,'error':{'code':-32603,'message':'unavailable'}}
                    if mutation == 'result':value['result']='0x'
                    if mutation == 'missing':value.pop('result',None)
                    if mutation == 'id':value['id']=True
                    if mutation == 'both':value['error']={'code':-32000,'message':'ambiguous'}
                return value
            with self.assertRaises(h.Failure):probe.run_probe(call,SENDER)
            self.assertEqual(len(self.calls),17)

    def test_wrong_chain_stops_before_state_or_cases(self):
        calls=[]
        def call(method,params):
            calls.append(method)
            return {'jsonrpc':'2.0','id':1,'result':'0x1'}
        with self.assertRaisesRegex(h.Failure,'RC4_RPC_CHAIN_ID'):probe.run_probe(call,SENDER)
        self.assertEqual(calls,['eth_chainId'])

    def test_state_change_is_not_success(self):
        source=self.source()
        def call(method,params):
            value=source(method,params)
            if len(self.calls)>37 and method=='eth_getTransactionCount':value['result']='0x1'
            return value
        with self.assertRaisesRegex(h.Failure,'RC4_RPC_ACCOUNT_STATE_CHANGED'):probe.run_probe(call,SENDER)
        self.assertEqual(len(self.calls),52)

    def test_deadline_is_checked_after_each_request(self):
        source=self.source();elapsed=[0]
        def call(method,params):
            value=source(method,params);elapsed[0]=121;return value
        with self.assertRaisesRegex(h.Failure,'RC4_RPC_DEADLINE'):
            probe.run_probe(call,SENDER,clock=lambda:elapsed[0])
        self.assertEqual(len(self.calls),1)

    def test_sender_must_be_canonical_before_any_call(self):
        for sender in ('', '0x12', '0x'+'AB'*20, None):
            with self.assertRaisesRegex(h.Failure,'RC4_RPC_SENDER_INVALID'):
                probe.run_probe(lambda *_:self.fail('must not call'),sender)

    def test_state_error_or_invalid_quantity_is_not_absence(self):
        for invalid in ({'error':{'code':-32000,'message':'missing'}},{'result':'0x00'}):
            source=self.source()
            def call(method,params):
                value=source(method,params)
                return {'jsonrpc':'2.0','id':1,**invalid} if method=='eth_getBalance' else value
            with self.assertRaises(h.Failure):probe.run_probe(call,SENDER)
            self.assertEqual(len(self.calls),2)

    def test_failure_preserves_partial_evidence_and_exact_count(self):
        source=self.source();record={}
        def call(method,params):
            value=source(method,params)
            if len(self.calls)==19:value={'jsonrpc':'2.0','id':1,'result':'0x'}
            return value
        with self.assertRaises(h.Failure):probe.run_probe(call,SENDER,record=record)
        self.assertEqual(record['outcome'],'INCOMPLETE')
        self.assertEqual(record['requestCount'],19)
        self.assertEqual([c['outcome'] for c in record['cases']],['PASS','PASS','FAIL'])
        self.assertEqual(len(record['lastResponseSha256']),64)
        self.assertEqual(record['automaticRetries'],0)

    def test_float_error_code_cannot_match_integer_contract(self):
        source=self.source();record={}
        def call(method,params):
            value=source(method,params)
            if 'error' in value:value['error']['code']=float(value['error']['code'])
            return value
        with self.assertRaisesRegex(h.Failure,'RC4_RPC_ERROR_CONTRACT'):
            probe.run_probe(call,SENDER,record=record)
        self.assertEqual(record['requestCount'],19)
        self.assertEqual(len(record['cases']),2)

    def test_transport_failure_keeps_counts_without_exposing_exception_text(self):
        record={}
        def call(*_):raise OSError('PRIVATE-REMOTE-DETAIL')
        with self.assertRaises(OSError):probe.run_probe(call,SENDER,record=record)
        self.assertEqual(record['requestCount'],1)
        self.assertEqual(record['reason'],'RC4_RPC_OSERROR')
        self.assertNotIn('PRIVATE',str(record))


if __name__ == '__main__':unittest.main()
