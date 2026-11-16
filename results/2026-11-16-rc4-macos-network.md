# RC4-06 — 고정 RC.4 동종망과 독립 Mac 관리 CLI 전환 인수

- 날짜: 2026-11-16
- 판정: **새 4-validator 동종망·복구·wrong-pin, 실행 중 관리 CLI 전환, 선정 RPC 인수 PASS. 독립 종료·RPC 감사 PASS.**
- 근거: [정제 JSON](./2026-11-16-rc4-macos-network.json) · [RC.4 도입 계획](../design/2026-11-15-rc4-adoption-plan.md) · [인수 도구](../tests/acceptance/README.md)

[RC4-05 단일 노드 서비스 인수](./2026-11-15-rc4-macos-service.md)를 커밋한 clean source `62f138941f0650a085e88e08d7db057d228a34b7`에서 관리 CLI C를 준비했다. 고정 runtime R과 내부 실행기 A는 유지하고, 새 관리 home·control·DB로 RC.4 동종 4-validator mTLS 망을 실행했다. 제품 Rust·인수 하네스·NIGO 코드를 변경하지 않았다.

실제 네트워크 시나리오는 **1회, 525.713895125초, exit0**, 자동 재시도는0회다. 설치·명시 초기화5회, 서비스 run·정상 stop 각12회, 서로 다른 거래8개·제출9회이며 최대 동시 노드는4개다. 다섯 번째 설치는 별도 wrong-pin 인스턴스다. 환경은 macOS26.6.2 arm64, 로그인한 사용자의 LaunchAgent다. 시간은 이 시험의 관측값이며 성능 인수 기준이 아니다.

## 고정한 runtime과 두 관리 CLI

| 구성 | 실제 source / SHA-256 |
| --- | --- |
| 관리 CLI B source | `65fe83f0d569fcf2231474342fa0d60ca72555d5` |
| B binary | `f08578809744d21313aad3c8cb39644107af67d5558661d2ecc9d81f2edb138c` |
| B signed CLI-only bundle | `0b03f7324c6b2b735aed9ab7793e65d151af1445f93e0506068289d19cb03a23` |
| 관리 CLI C source | `62f138941f0650a085e88e08d7db057d228a34b7` |
| C binary | `f12110d0644042c1cb97482cf68943e0dcf05b09bcc9514e25e2c26df0f8a153` |
| C signed CLI-only bundle | `847588c78c5bca01745a1113548a2e5113679d0bc41a9c2be2eb5430af4249d5` |
| 내부 실행기 A source | `d38027a474f1e8d4fb7b1d38ea17972ba88d93af` |
| A binary / worker | `d2d5c586bf2ab59ac3198cbcc0f89101fc18d31e6f4ec7812b428e481a63f647` |
| RC.4 runtime R archive | `a9f67988e785260b35504bd79a19df4a4820e72eafbf537328ca2e31f7e16f60` |
| R manifest | `fe21e898f5ace8be57ad838620f14da1c3e07b3ffcfbcb969fd63a67854a142e` |
| NIGO RC.4 source | `037c98c315d5322fa7da266ca0254d61f3863622` |
| NIGO JAR | `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a` |
| Temurin Java launcher | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |

R은 [RC4-03](./2026-11-15-rc4-runtime-assembly.md), B는 [RC4-04](./2026-11-15-rc4-cli-harness.md) 공급물을 재사용했다. profile은 `BXDL_RC4_EXECUTOR_V1`, JRE는 Temurin21.0.12.1+1-LTS다. R/A/JAR/JRE를 재빌드하거나 교체하지 않았다. 다섯 설치본 각각 payload269파일·326,373,420bytes, 그중 JRE259파일의 membership·hash·size·mode가 일치했다. worker12개 모두 A와 같은 bytes였다.

C는 실제 clean source에서 만든 별도 서명 배포본이다. C의4개 payload·4,248,679bytes와 manifest, 외부 Ed25519 서명을 검증했으며 RC4-04의 시험용 서명키를 재사용했다. B/C의 Rust 제품 구현·빌드 입력·자동완성·배포 도구는 같고 실제 source revision과 관리 가이드가 다르다. 따라서 이번 결과는 독립 배포본 선택·복귀와 실행 중 노드 보존을 검증한 것이며, 서로 다른 제품 동작 사이의 업그레이드 호환성을 입증한 것은 아니다.

## 입력 보존과 실행 중 CLI 전환

RC4-02의 원본 fresh fixture를 사용했다. 준비된 Type2 거래12개 중 nonce0~7을 사용했으며 새 credential을 발급하지 않았다. 원본 fixture **67파일**의 hash·size·mode·inode·device·mtime/ctime가 실행 전후 및 사후 감사에서 같았다. 원본 fixture에 노드 data 디렉터리를 만들지 않았고 기존 준비 상태 요약도 보존됐다. RC.3 control·DB는 사용하지 않았다.

관리 명령은 `cli install`로 만든 안정된 진입점을 사용했다. 실행 중 전환은 다음 순서였다.

| 전환 | 실행 중 정상 노드 | 전후 대조 |
| --- | --- | --- |
| B→C update | 4 | attempt·nodeInstanceId·PID, data 디렉터리 identity, binding·설정·초기화 journal·worker 동일 |
| C→B rollback | 4 | 동일 |
| B→C update | 3 | 동일 |

전환 때문에 노드를 재시작하지 않았다. 직접 기록된 관리 명령은84회이며 위임된 실행기·JVM을 포함한 전체 프로세스 호출 수와 구분한다. 최종 진입점은 C를 선택했다. 독립 감사는 안정된 entry→current activation→release admission→manifest/payload 연결과 보관된 B/C 두 release를 검증했다. 전환 시점은 frozen runner의 전후 snapshot을 근거로 하며, 독립 감사는 보존된 snapshot과 종료 후 파일을 재대조했다.

init report5개와 run report12개는 모두0600이었다. 각 보고서의 전체 build identity가 engine lock의 `expected`와 일치했고, init은 모두 `INITIALIZED`, run은 모두 `STOPPED`로 끝났다. 매 run의 새 nodeInstanceId, RUNNING의 genesis/node identity/chain fingerprint, 정상 종료의 `storageClosed`·`consensusStopReturned`도 확인했다.

## 거래 확정·catch-up·두 복구

서로 다른 거래8개에 대해 receipt identity, RPC block의 거래 포함 여부·hash·stateRoot, monitor canonical header/state를 검증한 원자료를 보존했다. 제출9회 모두 `ACKNOWLEDGED`였으며 nonce5만 같은 bytes로 한 번 명시 재제출했다.

| nonce | 인수 과업 | 확인 노드 수 / 확정 높이 |
| --- | --- | --- |
| 0 | 4/4 정상 확정 | 4 / 1 |
| 1 | n4 정지 중 3/4 확정 | 3 / 2 |
| 2 | n4 같은 DB catch-up 이후 새 거래 | 4 / 3 |
| 3 | 2/4 무정족수 뒤 3/4 복구, 원 거래 무재제출 | 3 / 4 |
| 4 | 네 번째 노드 복구·catch-up 이후 새 거래 | 4 / 5 |
| 5 | 별도 round skew 뒤 전체 노드 같은 DB 재시작, 명시 재제출1회 | 4 / 6 |
| 6 | 같은 DB 전체 재시작 뒤 후속 거래 | 4 / 7 |
| 7 | wrong-pin 격리 중 정상 정족수 | 3 / 8 |

두 canonical catch-up은 `SNAPSHOT_INSTALLS_HEADER_AND_STATE_WITHOUT_HISTORICAL_BODY` 계약을 따른다. 높이2에서 n4, 높이4에서 n3의 과거 receipt가 없었고 해당 사례에서는 이를 요구하지 않았다. runner는 네 노드의 canonical header/hash/stateRoot를 대조하고 존재하는 receipt만 검증했다. **과거 거래의 receipt와 block membership이 네 노드 모두에 존재했다고 주장하지 않는다.** catch-up HTTP 원응답은 보관되지 않아 독립 감사는 저장된 canonical 요약과 고정한 판정 코드를 확인했다. 이후 새 거래 nonce2·4의 네 노드 확정은 별도의 receipt/block/proof 원자료로 대조했다.

무정족수 시험은 서로 독립된 두2/4 구간이다. 유효한 새 거래를 제출한 뒤 관측 표본마다 head·finalized 높이 유지와 receipt 부재를 확인했고 두 노드 모두 round가 진행했다.

| 구간 | 실제 대기 / 표본 | 관측한 view·round |
| --- | --- | --- |
| 정족수 복구용 nonce3 | 45.518708125초 / 85 | view4, round1→3 |
| 전체 재시작용 nonce5 | 45.173557292초 / 85 | view6, round0→3 |

표본 간 최대 간격은 각각0.545784초·0.54715초였으며 첫 표본부터 마지막 표본까지는44.982165초·44.645175초다. 따라서 이는45초 대기 구간의 유한 polling 근거이며 연속 추적이나 무기한 안전성 증명이 아니다.

첫 복구는 기존 두 노드를 유지한 채 정족수를 복원했고 nonce3을 **재제출하지 않고** 확정했다. 두 번째는 정지 노드의 view6/round0과 활성 노드의 view6/round3 차이를 만든 뒤 모든 노드를 정상 정지하고 같은 네 DB로 재시작했다. nonce5를 같은 bytes로 **한 번 명시 재제출**하여 확정하고 nonce6도 확정했다. 전체 재시작 뒤 pending 거래가 재제출 없이 살아남는다고 주장하지 않는다. data 디렉터리 device/inode·engine-instance·제품/native 설정은 유지됐으며 DB/round reset·엔진 교체는 없었다. 운용 중 DB/WAL 전체 bytes 고정이나 엔진 버전 migration 시험과 구분한다.

## wrong-pin 격리

가장 작은 node ID의 원 signer를 정상 정지한 뒤 별도 control·DB를 초기화한 fault 인스턴스를 시작했다. fault 인스턴스의 sync를 끄고 잘못된 서버 pin을 사용해 **outbound WRONG_PIN3건·peer0**을 확인했다. 같은 signer가 중복 실행되지 않았고 정상 세 validator는 nonce7을 높이8에서 확정했다. 전체 시험의 동시 노드 수는4개를 넘지 않았다.

## 선정 RPC21사례 / 52read

첫 네 노드 기동과 B→C 전환 뒤, 첫 거래 제출 전에 C로 관리 중인 망에 인수 하네스가 RPC probe를 실행했다. 기능 사례는21개, 읽기 요청은52회, 소요 시간은0.202061292초로120초 상한 안이었다. 요청은 chain ID1회 → 전 상태15회 → 기능21회 → 후 상태15회 순서다.

| 기능 묶음 | 사례 수 |
| --- | --- |
| KZG 정상 call/estimate, 잘못된 proof 및 길이 거부 | 5 |
| public host `0x0105`~`0x0107`의 empty·host-read call/estimate | 12 |
| SHA-256 대조군 call/estimate | 2 |
| Type4 빈 authorization 거부 call/estimate | 2 |

오류 envelope·자료형·기대 응답과 관측한5계정의 `latest` balance/nonce/code 전후 일치를 확인했다. 모든52개 저장 요청·응답과 최종 probe 요약을 독립 RPC 감사로 대조했다. 전후 상태 동일성은 이5계정에 한정되며 전체 storage나 성공한 authorization simulation의 격리까지 검증한 것은 아니다.

이 **RPC subset의 거래 제출0회·자동 재시도0회**는 위 네트워크 시나리오의 거래8개·제출9회와 구분한다. 정상 signed Type4 실행, 성공 authorization simulation, KZG provider fault, nested host, Native 전체 recipe 및 Console 기능은 NOT_RUN이다. 독립 감사는 저장 원자료와 고정한 기대 사례를 검토했으며 새 RPC를 호출하거나 별도 KZG 검증기를 실행하지 않았다. 호출 순서는 고정 코드·저장 ordinal 근거이며 packet trace는 없다.

## 정상 종료·독립 감사와 보존한 오류 이력

| 종료 관측 | 결과 |
| --- | --- |
| 정상 stop | 12회 |
| runner가 확인한 기록 PID | 106개 모두 부재 |
| 독립 감사 기록 PID | runner PID를 포함한107개 모두 부재 |
| LaunchAgent job | 12개 모두 NOT_LOADED |
| 시험 TCP LISTEN | 8포트 모두 없음 |
| 시험 root의 남은 제품 프로세스 | 0 |
| 기존 GUI/user/system 관련 job·설치 plist 기준선 | 동일 |

독립 종료 감사는 실행 전후 fixture67파일, 하네스9파일, 다섯 payload와 worker12개, init/run report 전체 identity, 전환 snapshot, 거래 원자료, 복구와 최종 OS 상태를 읽기 전용으로 대조했다. credential·signed transaction은 private 환경에서 content hash만 대조하고 내용을 출력하지 않았다. 감사로 추가한 제품 CLI/JVM/서비스 실행·변경과 RPC 호출은 모두0회다.

두 감사 도구의 첫 판정 오류도 원자료로 보존했다.

- 사전 감사 첫 `INCOMPLETE / LEAF_PIN_MISMATCH`: 감사식이 transport domain separator를 빠뜨리고 raw DER SHA를 비교했다. 기존 `prepare_fixture.py`의 정확한 pin 규칙으로 감사식만 수정해 별도 사전 감사에서 PASS했다. fixture나 인증서를 바꾸지 않았다.
- 종료 감사 첫 `INCOMPLETE / REGULAR_FILE_REQUIRED`: 의도된 안정된 CLI entry symlink에 일반 파일 조건을 적용했다. 정확한 symlink 경로·activation·release admission·manifest/payload 연결을 검증한 뒤 resolved 일반 파일 hash를 비교하도록 감사만 수정해 별도 종료 감사에서 PASS했다. 최초 감사·v1 script·수정 diff를 보존했다.

**제품 시나리오는 재시도하지 않았고 첫 실행의 PASS 결과를 수정하지 않았다.** 감사 보정 전후에도 서비스 재기동·RPC 재호출·제품 판정 조건 완화는 없었다.

## 자동 검사 이력과 근거 범위

이번에 `make check`를 다시 실행하지 않았다. RC4-05에서 검사한 source136파일의 hash가 C의 clean source와 같음을 실행 전 wrapper와 결과 작성 시 다시 확인했다. 실행한 하네스9파일도 clean source·저장소·실행 전후 hash가 같다.

| 검사 | 이번 기록 |
| --- | --- |
| 실제 RC4-06 네트워크 / 종료 감사 / RPC 감사 | PASS / PASS / PASS |
| `make check` | 신규 실행 없음; 이전 검사 source136파일과 일치 |
| Rust596 PASS·Python163 PASS/기존 opt-in2 SKIP·fmt/Clippy PASS | **RC4-05의 이전 실행 이력** |
| Linux 타입 검사 / Linux runtime | 이번 재실행 없음 / NOT_RUN |

선택한 private 근거306파일의 inventory SHA-256은 `d1be1de3bfcb05f7f5dd3890cc0b28d8a8643b6542d136dfb5451edac513d5b2`다. 목록 자체와 원 stdout/stderr·snapshot·보고서는 private 위치에 보존했다. inventory에는 DB·키 파일·archive를 넣지 않았다. 주요 실행 결과 SHA-256은 `72361b557c3f41c948c8a9aaa75d1590b0a42fe2e20a17c9c3890e5e625c3e8c`, 최종 독립 감사는 `e3578365151ad9d0d42308b511dea9a350828a5292c2e9195d3d9b09ccd836bc`다. [JSON](./2026-11-16-rc4-macos-network.json)에 주요 근거 digest·크기와 source pin을 기록했다. 자격증명·raw signed transaction·DB/WAL·archive·개인 경로·PID 목록을 Git에 넣지 않는다.

## 다음 범위

**RC4-06은 DONE**이다. 이 결과와 JSON을 고정한 시점의 후속 순서는 RC4-07 NIGO 정식 인수 회신과 RC4-08 CLI05-04 실제 사람 과업이다. 이후 RC4-07 회신은 [NIGO PR #257](https://github.com/bwg-loonshots/nigo-protocol/pull/257)로 머지했으며 현재 다음은 RC4-08이다. 근거 JSON의 `nextStage`는 결과 고정 당시의 순서를 보존한다. 다음 계획을 막는 새 NIGO 기능/API/JAR 선행 구현은 확인되지 않았다.

이번 PASS는 지정 Mac의 새 동종 개발 망과 선정 RPC 범위다. 기존 DB migration·혼합 버전 rolling upgrade·multi-host, crash/power loss·로그아웃·sleep/wake, 빈 setup 전체 문답·EOF/HUP·실제 사용자 편의성, REM-01·A7·Linux/systemd·Docker·LaunchDaemon·장기/성능·전체 G1-M/J5는 별도다. development·officialRelease=false·계약 PROPOSED·REQ-0002/0003 OPEN·고객 납품 NOT_GRANTED를 유지한다. 기존 RC.3 지원·과거 결과와 RC4-05 당시 범위도 보존한다.
