# macOS 동일 패키지의 4-validator 인수

2026-10-20 · BXDL `fecf7a622b06c867fac14eed74f26a66cb0a62c8` · [정제 JSON](./2026-10-20-macos-qbft-package.json) · [재현 도구](../tests/acceptance/README.md)

## 판정

동일 서명 archive의 네 설치본으로 실제 거래 확정, 3/4 정족수의 지속 처리, 같은 DB를 사용하는 노드의 catch-up과 재합류 후 새 거래 확정을 확인했다. 2/4 상태를 45초 유지하는 동안 새 거래가 확정되지 않는 것도 확인했다. **전체 인수는 INCOMPLETE**다. 정족수 복구 이후의 결과와 독립 인증 시험은 아래 실행별 근거로 구분한다. 로컬 READY와 합의 검증을 혼동하지 않으며 BXDL CLI의 `globalConsensus=NOT_CHECKED`도 그대로 유지했다.

## 고정한 공급물

| 항목 | 값 |
| --- | --- |
| BXDL commit | `fecf7a622b06c867fac14eed74f26a66cb0a62c8` |
| CLI SHA256 | `120089de764575709fe5d708a2d502df7dafd0b659ec4086c1176bfb8c0a803d` |
| NIGO source | `540a89f0814c01d2a4ec67da1087af35195882ca` |
| NIGO JAR SHA256 | `f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45` |
| Java launcher SHA256 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |
| signed archive SHA256 | `9edee7c322cb975f402b9715a2d49386b70e00629c2bef453db9ac3ac90becea` |
| manifest SHA256 | `4f833ea042a8779103d1e7ab70b322eb768a4e2ea7bbbdcb4ccf399b4bdc8b99` |
| 실행 환경 | macOS 26.6.2 (25G83), arm64, 32 GiB RAM |
| runtime | Temurin 21.0.12.1+1-LTS full JRE, 전체 259파일 대조 |

새 Ed25519 시험키로 archive를 서명했고 외부 공개키로 검증했다. 정식 배포키·Apple 서명·notarization 근거는 아니다. 고정 JAR에 이미 포함된 `QbftDevnetGenerator`로 새 시험용 NGVK 네 개와 개별 TLS/공통 신뢰 자료를 생성했다. leaf 인증서의 NIGO domain-separated pin을 manifest와 대조했다. 고정 JAR를 재빌드하거나 NIGO 엔진 코드를 수정하지 않았다.

Node 22.15.0·ethers 6.17.0은 준비 단계에서 새 지갑과 nonce 0~7 Type-2 거래를 오프라인 서명하는 데만 사용했다. chain은 QBFT/DEV_QBFT/11578, ZERO fee, empty block 비활성화, request timeout 5초·상한 20초다. 새 지갑만 funding했다. 네트워크 준비는 실제 PTY를 사용했고 이후 install/register/init/start/stop은 CLI를 직접 호출했다. 이번 실행을 설치 마법사 전체 PTY 인수로 표시하지 않는다.

## 실행별 근거

| 실행 | 결과 | 의미 |
| --- | --- | --- |
| A | INCOMPLETE | 축약한 BXDL revision을 입력해 package build가 거부됐다. 노드는 시작하지 않았으며 전체 40자리 commit으로 입력을 바로잡았다. |
| B | 일부 PASS / 전체 INCOMPLETE | 새 archive의 4-node 거래를 높이 1에서 확정하고, n4 정지 중 나머지 세 노드가 높이 2를 확정했다. n4의 과거 ETH receipt를 요구한 catch-up 조건에서 timeout했다. |
| C | 일부 PASS / 전체 INCOMPLETE | 같은 archive/control/data의 완전 정지를 확인한 뒤 이어 실행했다. 네 노드의 높이 2 hash/root가 같고 재합류 후 새 거래가 높이 3에서 모두 확정됐다. 2/4에서 45초·81표본 동안 새 확정이 없고 round가 증가했다. n4를 복구한 뒤 원 거래가 90초 내 확정되지 않았다. |
| D | PASS | 같은 archive와 별도의 새 키·data로 wrong-pin 시험을 수행했다. 부정 노드는 peer 0·outbound WRONG_PIN 9회, 정상 세 노드는 각각 TLS1.3 peer 2개·quorum 3과 새 거래 확정을 확인했다. |
| E | INCOMPLETE | C 이후 같은 DB에서 네 노드를 함께 재시작하고 원 거래를 한 번 명시 재제출했다. 네 노드가 서로 연결되고 거래를 보유했지만 90초 안에 새 확정이 없었다. 기준 거래 실패로 다음 nonce 4·새 45초 시험은 진행하지 않았다. |

A/B/C/D/E의 `result.json`은 원래 판정 그대로 보존했다. C가 B의 관측 조건을 보완했다고 B를 PASS로 변경하지 않았다. C의 2/4→3/4 관측은 거래 재제출 없이 진행했다. 단일 노드 READY만으로 네트워크 복구를 선언하지 않는다.

## Catch-up과 과거 receipt

RC1의 snapshot 설치는 historical header/certificate/checkpoint와 상태를 복원하며 과거 transaction body 전체를 복원하지 않는다. `HistoricalFinalizedLedgerInstaller.java`, `BlockStore.java`, `nigo-storage-rocksdb/STORAGE_LAYOUT.md`와 receipt 조회 경로를 확인했다. 이 production 코드와 아래 공급자 시험은 읽은 checkout `6489c9540853bdadfa8c384227558d5eeca3ffea`에서 RC1 source 대비 변경이 없음을 확인했다.

C에서 높이 2의 block hash/state root는 네 노드가 같았고 receipt 존재 여부는 `[true, true, true, false]`였다. 이는 로컬 historical body 가용성의 차이로 기록한다. 재합류 후 새 거래에는 네 노드 모두의 성공 receipt와 동일 높이/hash/root를 요구했고 높이 3에서 통과했다. 운영 문서에도 과거 receipt의 `null`이 곧 미확정을 뜻하지 않는다는 제한을 반영한다.

## 공급자 시험과 복구 조건의 차이

고정 source의 `nigo-e2e-test-suite/tests/04-qbft-multiprocess.spec.ts`와 `nigo-java/nigo-node/QBFT_DEVNET_RUNBOOK.md`의 quorum-loss 시나리오를 확인했다. 공급자는 첫 round 증가를 최대 45초 기다린 뒤 즉시 세 번째 노드를 복구하고, 그 노드에 같은 signed transaction을 재제출한다. 45초 내내 정족수 부족을 유지하는 시험은 아니다. 또한 reconnect 이전 pending backfill·전체 mempool reconciliation은 보장하지 않는다.

C의 90초 정체만으로 합의 결함을 확정할 수 없으므로, 별도 E probe에서 명시 재제출과 `/monitor/api/consensus/progress`를 함께 관측했다. 코드 검토상 `QbftStateMachine.enterRoundIfQuorum`은 높은 round 진입에 먼저 3표를 요구하고, timeout은 자기 round를 하나씩 증가시킨다. 2/4 동안 앞선 두 노드와 재시작 노드 사이의 round 격차가 유지될 가능성이 있으나, 소스 추론과 실제 관측을 구분한다.

## 복구 probe의 관측과 NIGO 확인 사항

E는 전체 115표본 중 재제출 후 114표본을 남겼다. 이 114표본 모두에서 네 노드가 peer 3·latest/finalized 높이 3·receipt null이었고, 113표본에서 네 노드 모두 pending 1이었다. 마지막 관측은 다음과 같다. 노드별 HTTP 응답은 순차/병렬 요청의 관측이며 원자적 전역 snapshot은 아니다.

| 노드 | height / round | 단계 | round-change 표 | prepare / commit | pending / peer |
| --- | --- | --- | --- | --- | --- |
| n1, n2 | 4 / 12 | ROUND_CHANGE | 2 | null / null | 1 / 3 |
| n3, n4 | 4 / 9 | PREPARE | 4 | 2 / 0 | 1 / 3 |

거래가 네 노드에 전달된 뒤에도 확정이 진행되지 않아 **단순 pending 재전파 누락만으로는 설명할 수 없는 합의 라운드 복구 문제**로 분류한다. 90초 내 미복구는 확인했으며 영구 정체 또는 정확한 수정 위치를 확정하지 않는다. DB·round 초기화, 강제 kill·live bootout으로 성공 상태를 만들지 않았다. 모든 실행은 정상 종료 근거를 남겼고 최종 D/E의 여덟 시험 인스턴스도 STOPPED를 확인했다.

NIGO에 다음 확인이 필요하다.

1. 고정 RC1에서 2/4를 45초 유지한 뒤 3/4 복구, 이어 같은 DB의 4/4 재시작·명시 재제출 시 라운드 수렴을 재현하고 회귀를 추가한다. 높은 round 두 표를 가진 노드와 뒤처진 노드가 만나는 경로 및 영속 safety round 복원을 검토한다.
2. DB·키·확정 원장을 보존하는 지원 복구 절차와 기대 복구 시간을 회신한다. 단순 전체 재시작과 거래 재제출은 이번 E에서 90초 내 복구되지 않았다. BXDL이 안전 상태를 초기화하는 우회 기능은 추가하지 않는다.
3. 엔진 수정이 필요하면 기존 RC1 bytes를 바꾸지 않고 새 불변 후보와 source/JAR SHA·시험 근거를 공급한다. BXDL은 수신·lock 갱신·같은 Mac package 재인수 뒤에 복구 항목을 닫는다.

이 확인 요청은 BXDL 결과 문서에 정리했다. 공유 NIGO checkout·요구사항 원장은 이번 작업에서 수정하지 않았다.

D의 추가 읽기 검사는 정상 세 노드 모두에서 요청 transaction hash, 성공/Type-2 receipt, RPC/monitor 높이, block 내 거래 포함과 hash/root를 독립 대조했다. D는 별도의 새 네트워크 시험이며 B/C/E의 복구 실패를 덮어쓰지 않는다.

## 검증 도구와 공개 범위

B~E에 사용한 하네스 bytes와 A~E 원 결과 hash를 private evidence에 보존했다. package build에서 거부된 A의 하네스 snapshot은 남아 있지 않으며 JSON에도 빈 목록으로 기록했다. 저장소 수록본은 receipt의 요청 hash/type·block 높이·거래 포함 대조, present historical receipt 검사와 finality timeout 진단을 보완했다. 실행 결과를 이 보완본 전체의 재실행으로 소급하지 않는다. fixture·거래·lock 입력 고정과 legacy B 재개의 정확한 allowlist, init 불명 결과의 미정리 기록, 경로 정규화도 추가했다. Rust 제품 source와 공급 JAR/JRE lock은 그대로다.

최종 `make check`는 fmt·Clippy 및 **343 PASS / 0 failed / 0 ignored**다. 처음 제한 환경에서는 loopback·특수 파일 등 권한 제약으로 5개가 실패했고, 같은 source를 정상 권한 환경에서 다시 검사해 전체 통과했다. Python 판정 회귀 9개, syntax와 실제 fixture 입력·legacy B 대조도 통과했다. 입력 guard의 잘못된 목적·endpoint·경로·hash·거래 등 음성 10사례를 별도로 확인했다. 이 제품 검사와 Python 하네스의 구조/판정 검사는 실제 네트워크 복구 성공을 대신하지 않는다.

키·비밀번호·signed transaction 원문·설정·DB·native 로그·archive는 Git에 넣지 않는다. 정제 JSON에는 고정 artifact/hash, 실행 판정과 필요한 관측만 포함한다. 시험 자료와 원문은 로컬 private 시험 폴더에 보존하며 고객 자료는 사용하지 않았다.

## 남은 인수

전체 G1-M은 열어 둔다. Mac 로그인/로그아웃·sleep/wake·실제 중단/실패 경계·안전한 제거, 최소 macOS와 J5 공급망 자료, 정식 릴리스 pipeline은 별도 작업이다. 이번 시험은 loopback의 같은 UID·단일 Mac 구성으로, multi-host·Linux/systemd·Docker·LaunchDaemon·장기 운영 또는 production 승인을 의미하지 않는다. 실행 중 인터넷을 차단·계측하지 않았으므로 ART-01 전체 완료로 확대하지 않는다.
