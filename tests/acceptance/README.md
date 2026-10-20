# BXDL Mac 4-validator 패키지 인수 도구

이 도구는 새 시험 자료와 격리된 로컬 인스턴스로 BXDL 패키지를 검증한다. 고객 설치기나 고객용 키·CA 발급기가 아니다. 이 문서는 실행 절차와 판정 범위를 설명하며, 현재 시험의 PASS를 선언하지 않는다.

첫 대상은 로그인된 GUI 세션이 있는 macOS arm64, RocksDB, 사용자 LaunchAgent다. HTTP와 P2P를 모두 `127.0.0.1`에 두고 노드별 포트를 분리한다. 같은 사용자 UID의 네 인스턴스이며, 서로 다른 호스트나 강한 보안 격리를 검증하는 구성이 아니다.

## 도구와 실행 경계

| 파일 | 역할 | 실행하지 않는 것 |
| --- | --- | --- |
| `prepare_fixture.py` | 고정 RC1 JAR의 `QbftDevnetGenerator`로 새 시험용 NGVK·개별 mTLS·신뢰 자료 생성, 공개 목록과 제품 설정 작성, 새 거래 지갑과 미리 서명한 Type-2 거래 준비 | BXDL 설치·등록·초기화·노드 기동·거래 제출 |
| `run_acceptance.py` | 새 시험 서명키로 실제 archive 조립·검증, 같은 archive의 네 설치본 준비, 네트워크 설정 도우미·등록·초기화·LaunchAgent·거래/합의 관측 | 고객 인스턴스 인수, 기존 설치본 덮어쓰기, 운영 키 발급 |
| `test_oracles.py` | 잘못된 receipt/block의 거짓 성공, historical receipt 제한, init 불명 cleanup의 오프라인 회귀 | 실제 엔진·서비스 실행 |
| `fixture_guard.py` | 고정 RC1 시험 fixture의 identity·loopback·파일 범위·입력 hash와 legacy B 재개 allowlist 검사 | credential 내용 읽기·암호학적 거래 재검증 |
| `support.py` | 새 파일 저장·복사, 실제 PTY 문답, CLI JSON 수집, 제한된 상태 검증과 비공개 근거 기록 | 독자적인 복구·서비스 제어 정책, import 시 실행 |
| `run_pin_only.py` | 보존한 서명 archive와 새 fixture에서 잘못된 pin 거부·정상 3-node 합의를 독립 확인 | 앞선 catch-up·정족수 복구 실패의 성공 전환 |
| `run_recovery_probe.py` | 보존한 B/C 실행의 정지 근거를 확인하고, 같은 DB에서 거래 명시 재제출과 합의 라운드 관측 | 범용 복구·DB 초기화·자동 재전송 |

준비 단계에도 고정 JAR의 Java 도구는 실행한다. 그러나 노드를 시작하거나 노드 DB를 초기화하지 않는다. 준비 결과의 `PREPARED_NOT_EXECUTED`는 자료 준비만 뜻한다. 빈 `data` 상위 폴더는 만들지만 노드별 data는 생성하지 않는다.

Node와 ethers는 준비 단계의 오프라인 거래 서명에만 필요하다. 실행 단계는 준비된 거래 bytes를 읽고 설치된 CLI/JRE/JAR를 사용한다. NIGO checkout·Gradle·Node/ethers를 노드 실행 중 호출하지 않는다. 다만 인터넷 차단과 시스템 호출을 계측하는 하네스는 아니므로, 이를 네트워크 차단 상태의 완전한 source-free 인수 결과로 표현하지 않는다.

## 고정 공급물과 사전 조건

현재 준비기는 다음 조합에만 맞춰져 있다. hash가 다른 JAR/Java를 최신 버전이라는 이유로 대신 넣지 않는다.

| 대상 | 고정 값 |
| --- | --- |
| NIGO source | `540a89f0814c01d2a4ec67da1087af35195882ca` |
| RC1 JAR SHA-256 | `f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45` |
| Temurin Java launcher SHA-256 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |
| 선택 runtime | Temurin `21.0.12.1+1-LTS`, macOS arm64 full JRE |
| 준비용 ethers | `6.17.0` |
| 시험 chain/profile | `11578` / `DEV_QBFT`, ZERO fee |

Python 3와 준비용 Node/ethers는 미리 확보한다. 실행에는 macOS arm64용 BXDL CLI, 검증한 `engine`·`runtime`·`docs`·`licenses` payload, 대응 engine lock과 package spec, Ed25519를 지원하는 OpenSSL이 필요하다. `docs/runtime-provenance.json`의 전체 runtime 경로·크기·mode·hash를 실행 도구가 대조한다. CLI bytes와 `--revision`으로 지정한 전체 40자리 commit의 대응 관계는 실행자가 먼저 확인한다.

새 시험은 기존에 없는 fixture와 실행 출력 폴더를 사용하고 부모 폴더는 미리 준비한다. 네 HTTP 포트와 네 P2P 포트가 서로 겹치지 않고 비어 있는지 확인한다. 고객 키·고객 control/data·사용 중인 서비스 경로를 입력하지 않는다. 아래의 제한된 `--continue-from`만 완전히 정지한 이전 시험의 control/data를 이어 사용한다. LaunchAgent에서 접근 가능한 시험 위치를 선택하며, macOS 접근 정책을 우회하기 위해 기존 사용자 설정을 바꾸지 않는다.

이 도구의 새 Ed25519 키는 시험용 package 서명을 위한 것이다. 정식 배포 신뢰 키나 Apple Developer ID/notarization을 대신하지 않는다. 선정 Temurin의 upstream SBOM archive hash 불일치·서명/출처 확인과 전체 제품 NOTICE/SBOM·대응 source 전달은 별도 잔여 사항이다. runtime 실행 검증이나 이 하네스의 PASS가 J5·고객 출시 승인을 완료하지 않는다.

## 1. 새 시험 자료 준비

다음 경로는 예시용 placeholder다. 실제 검증한 로컬 파일과 비어 있는 시험 포트로 바꾼다.

```sh
python3 ./prepare_fixture.py \
  --java /path/to/verified-payload/runtime/bin/java \
  --jar /path/to/verified-payload/engine/nigo-node.jar \
  --node /path/to/node \
  --ethers-module /path/to/node_modules/ethers/lib.esm/index.js \
  --output /path/to/new-fixture \
  --http-base-port 28080 \
  --p2p-base-port 29090
```

준비기는 새 validator 네 개의 시험 자료, `chain.json`, `network.json`, `products/n1.json`부터 `n4.json`, 새 지갑, nonce 0~7의 미리 서명한 거래를 만든다. 자료는 새 private 폴더에 보관한다. 생성된 `application.yml`은 공급자 생성기 근거로 남기며, 실행 설정으로 자동 채택하지 않는다.

고정 keytool이 있으면 공개 leaf 인증서를 내보내 NIGO domain hash와 manifest pin을 대조한다. 없으면 `certificateExport=NOT_AVAILABLE`로 기록한다. 실패한 준비 폴더는 덮어쓰거나 재사용하지 않고 보존한다. 성공 여부와 생성 범위는 `fixture-summary.json`으로 확인한다.

시험 원장에는 새 거래 지갑만을 명시적으로 funding한다. 공개 devnet 고정 지갑이나 고객 자산을 사용하지 않는다. `ALLOWLISTED` 등록 정책의 빈 허용목록을 유지하며 이 시험에서 Native 모듈 게시를 수행하지 않는다.

## 2. 실제 패키지 설치·실행

`--run`을 생략하면 실행하지 않는다. 아래 명령은 package 생성·설치·초기화·서비스 시작과 거래 제출을 실제 수행한다.

```sh
python3 ./run_acceptance.py \
  --run \
  --cli /path/to/release/bxdl \
  --revision FULL_40_HEX_BXDL_COMMIT \
  --fixture /path/to/new-fixture \
  --payload-source /path/to/verified-payload \
  --lock /path/to/engine-lock.rc1-temurin-development.json \
  --spec /path/to/package-spec.json \
  --openssl /path/to/openssl \
  --output /path/to/new-acceptance-run
```

실행 도구는 CLI와 payload를 새 경로로 복사하고, 고정 JAR/Java와 runtime inventory를 확인한 뒤 하나의 서명 archive를 만든다. 네 정상 노드와 별도 부정 시험 노드는 모두 그 archive를 설치한다. 설치본의 CLI가 입력 CLI와 같은 bytes인지도 대조한다.

정상 네 노드의 `setup --network`는 실제 PTY에서 `PKCS12` / `PKCS12`, `join-network`, 합의 자동 시작 `yes`, archive `yes`를 선택하고 새 native JSON을 게시한다. 제품/native 입력을 등록한 뒤 명시 init을 호출한다. 등록 단계에서 data가 생성되지 않았는지, init 뒤 네 노드의 genesis가 같은지 확인한다.

처음 네 노드는 **모두 join-network와 archive를 켠 상태로 함께 시작**한다. 이후 같은 DB에서 뒤처진 노드를 재시작할 때 기존 입력을 수정하지 않고 archive 기반 catch-up을 사용하기 위한 시험 구성이다. 이는 고객 네트워크의 초기 배치 정책 전체를 대신하지 않는다.

시작 관측 기한이 끝나면 같은 attempt를 `status --wait`로 이어 관측한다. 새 start로 바꾸지 않는다. 로컬 `READY`와 BXDL의 `globalConsensus=NOT_CHECKED`는 그대로 유지하며, 네트워크 합의는 아래의 HTTP/RPC 근거로 별도 판정한다.

## 실제 실행이 다루는 시나리오

1. 네 validator가 같은 archive로 실행된 상태에서 Type-2 거래를 한 ingress에 한 번 제출한다. 각 노드의 성공 receipt, 확정 높이, 해당 높이의 block hash/state root를 RPC와 monitor에서 대조한다.
2. 네 번째 노드를 정상 종료하고 나머지 셋에서 다음 거래를 확정한다. 같은 DB/key로 네 번째 노드를 재시작해 새 attempt·nodeInstanceId, 동일 genesis와 놓친 높이의 canonical block hash/state root를 확인한다. 과거 receipt 존재 여부는 별도로 기록한다. 재합류 후 새 거래는 네 노드 모두의 성공 receipt와 같은 높이/hash/root를 요구한다.
3. 두 노드를 정상 종료한 뒤 두 생존 노드의 peer 수와 공통 확정 높이를 기준으로 삼는다. 새 유효 거래를 한 번 제출하고 45초 동안 receipt가 없고 latest/finalized 높이가 그대로인지, 같은 view height에서 round가 증가하는지 관측한다. 세 번째 노드를 복구해 원래 거래 hash의 성공 receipt와 확정을 확인한다. 네 번째 노드는 그 높이의 canonical hash/root로 catch-up을 확인하고, 다시 새 거래를 제출해 네 노드 모두의 성공 receipt를 요구한다. 유한 관측 구간의 no-finality 시험이다.
4. node ID가 가장 작은 정상 노드를 먼저 정상 종료한다. 같은 시험 identity를 별도 설치·control·새 data에서 사용하되 세 peer의 expected pin을 모두 틀리게 만든다. 이 부정 노드만 `new-network`로 설정해 sync를 끄고 archive는 켠다. catch-up 대기로 인증 거부 관측이 가려지지 않도록 한 별도 시험이며 정상 네 노드의 all-join 설정은 바꾸지 않는다. 해당 노드의 `OUTBOUND / CREDENTIAL_PIN / WRONG_PIN` audit와 peer 0, 정상 세 노드의 새 거래 확정을 확인한다. 동일 signer의 두 프로세스를 함께 시작하지 않는다.

현재 부정 시험은 잘못된 expected pin에 따른 격리다. 잘못된 role/validator, 만료·폐기 인증서, unknown CA 전체 행렬이나 부정 시험 뒤 원래 네 노드의 full-mesh 복원까지 완료했다고 표시하지 않는다.

기본 실행의 거래는 준비 단계에서 서명한 원 bytes를 한 번만 제출한다. 제출 응답 유실은 `UNKNOWN`으로 남기고 원 hash를 조회한다. 하네스가 다시 서명하거나 자동 재전송하지 않는다. 별도 복구 probe의 재제출은 아래에 명시한 진단 단계다. 서로 다른 노드의 finality certificate hash는 quorum seal 구성에 따라 다를 수 있으므로 동일성 조건에 넣지 않는다.

**시나리오 3은 RC1 공급자 시험보다 긴 정족수 부족과 재제출 없는 복구를 관측한다.** 공급자 `04-qbft-multiprocess.spec.ts`는 첫 round 증가를 최대 45초 기다린 뒤 바로 세 번째 노드를 복구하고, 그 노드에 같은 signed transaction을 명시적으로 재제출한다. `QBFT_DEVNET_RUNBOOK.md`는 reconnect 이전 pending backfill을 보장하지 않는다. 따라서 기본 실행의 복구 timeout 하나만으로 엔진의 합의 결함을 확정하지 않는다. 고정 45초 유지 뒤 재제출하는 별도 probe에서 거래 접수·pending·peer·round·vote를 함께 기록해 원인을 구분한다.

| 거래 nonce | 사용 단계 |
| --- | --- |
| 0 | 최초 네 노드의 거래 확정 |
| 1 | 네 번째 노드 정지 중 세 노드의 거래 확정과 이후 catch-up 기준 |
| 2 | 첫 재합류 후 네 노드의 새 거래 확정 |
| 3 | 2/4 관측 중 한 번 제출하고 quorum 복구 후 확정 |
| 4 | 2/4 시험 뒤 네 노드 복구 상태의 새 거래 확정 |
| 5 | wrong-pin 부정 노드 격리 중 정상 세 노드의 거래 확정 |

nonce 6~7은 준비된 예비 자료이며 현재 시나리오에서는 제출하지 않는다.

## Snapshot catch-up의 과거 receipt와 제한된 이어 실행

고정 RC1의 `HistoricalFinalizedLedgerInstaller`는 snapshot 이전의 역사 구간을 canonical header·certificate·checkpoint로 설치한다. state는 snapshot으로 가져오며, 그 구간의 transaction body와 과거 ETH receipt를 모두 복원하는 계약은 아니다. 같은 source의 `nigo-java/nigo-storage-rocksdb/STORAGE_LAYOUT.md`에서도 historical header와 body/receipt 가용성을 구분한다.

따라서 `canonical_at`은 대상 높이까지의 finality와 동일 block hash/state root를 요구하고, 과거 receipt 유무를 `historicalReceiptPresent`로 따로 기록한다. 이 경우 `historicalReceiptRequiredByThisCase=false`를 명시한다. 반면 재합류 뒤 실제로 처리하는 새 거래의 검사는 완화하지 않는다. 네 노드 모두의 성공 receipt와 같은 확정 block을 확인해야 한다.

이전 실행은 최초 4-node·3-node 거래 확정 뒤 snapshot catch-up 노드의 과거 ETH receipt를 기다리다 `FINALITY_TIMEOUT_same-db-catchup`으로 `INCOMPLETE`가 되었고 시험 노드들은 정상 종료했다. 이 기록은 유지한다. 과거 receipt까지 요구했던 하네스 조건을 RC1의 지원 범위와 구분하며, 새로운 이어 실행의 성공을 미리 선언하지 않는다.

`--continue-from`은 이 특정 실행을 이어 검증하는 경로다. 범용 resume·UNKNOWN 복구·결과 수동 채택 기능이 아니다. 다음 조건이 모두 필요하다.

- 이전 `result.json`이 `INCOMPLETE`, reason이 정확히 `FINALITY_TIMEOUT_same-db-catchup`이고 cleanup이 `remaining=[]`, `errors=[]`다.
- 이전 archive와 입력 CLI hash가 이전 결과에 고정된 값과 같고, 이전 서명 archive 검증이 통과한다.
- 기존 네 control의 현재 status도 정상 종료 근거를 만족한다. 남아 있는 실행을 강제로 중지하거나 기록을 고쳐 조건을 만들지 않는다.
- 이전과 같은 fixture·engine lock·CLI·revision을 지정하고, 결과를 저장할 출력 폴더만 새로 준비한다. 수록본은 B의 정확한 result/public/transaction/lock/revision hash allowlist로 이 제한을 검사한다.

```sh
python3 ./run_acceptance.py \
  --run \
  --continue-from /path/to/prior-incomplete-run \
  --cli /path/to/same-release/bxdl \
  --revision FULL_40_HEX_BXDL_COMMIT \
  --fixture /path/to/same-fixture \
  --payload-source /path/to/verified-payload \
  --lock /path/to/same-engine-lock.json \
  --spec /path/to/package-spec.json \
  --openssl /path/to/openssl \
  --output /path/to/new-continuation-run
```

현재 인자 형식은 이어 실행에서도 payload/spec/OpenSSL 경로를 요구한다. 그러나 기존 정상 네 노드의 archive를 재조립하거나 재설치·재초기화하지 않고 같은 설치본·control·data를 다시 사용한다. nonce 0·1을 재제출하지 않으며, 수정된 canonical catch-up 확인 뒤 nonce 2부터 남은 시나리오를 진행한다.

이전 `result.json`과 기존 evidence는 덮어쓰지 않는다. 새 결과는 `continuation.priorResultSha256`과 이전 판정·이유를 기록하고, 이전에 확인한 시나리오 결과와 이번 관측을 연결한다. 기존 control/data에는 명시적인 start/stop과 후속 거래에 따른 새 기록이 생긴다. 이어 실행은 처음부터 다시 수행한 독립 신규 설치 인수로 표시하지 않는다.

## 보존한 후보의 독립 진단

`run_pin_only.py`와 `run_recovery_probe.py`는 2026-10-20의 특정 archive·CLI hash를 코드에 고정한 재현 도구다. 임의 후보를 시험하는 범용 옵션으로 해석하지 않는다. 실제 입력과 필요한 이전 실행이 없으면 새 기본 실행부터 준비한다. 각 도구의 `--help`로 경로 인자를 확인하며 `--run`을 명시해야 실행한다.

Pin-only는 새 fixture와 HTTP 24420~24423·P2P 25420~25423을 사용한다. 기존 실행과 다른 identity를 확인하고 같은 archive에서 정상 세 노드와 잘못된 expected pin을 가진 한 노드를 설치한다. 정상 세 노드의 TLS1.3 인증과 거래 확정, 부정 노드의 outbound WRONG_PIN·peer 0을 확인한다. 기존 실행의 control/data는 재사용하지 않는다.

Recovery probe는 완전히 정지한 B/C 실행과 원 fixture A를 검사한다. 네 노드를 함께 재시작하고 이전 미확정 nonce 3의 같은 signed transaction을 한 번 명시 재제출하여 기준 높이를 확인한다. 이것이 실패하면 다음 거래로 넘어가지 않는다. 기준이 확인되면 두 노드를 정상 정지하고 nonce 4를 한 번 제출해 45초 정체를 관측한다. 세 번째 노드 복구 후 그 노드에 동일 bytes를 한 번 재제출하고 최대 90초 동안 canonical finality와 progress를 수집한다. 새 서명·무한 재전송·DB/round 초기화는 수행하지 않는다.

저장소 수록 전 `run_acceptance.py`의 receipt transaction hash/type·block 높이·거래 포함 검사를 강화하고, catch-up에서 존재하는 receipt도 대조하도록 보완했다. finality timeout에는 제한된 progress 근거를 추가했다. fixture·거래·lock 입력 pin, loopback·credential 참조 범위와 legacy B allowlist, init의 불명 결과를 cleanup 미정리로 남기는 처리도 추가했다. **B/C/D/E 실행에 사용한 원본과 이 보완본은 hash가 다르다.** 원 실행의 PASS를 보완본 전체 재실행으로 소급하지 않으며, 실행별 하네스 hash·추가 독립 검사는 [검증 기록](../../results/2026-10-20-macos-qbft-package.md)에 구분한다.

## 실패·종료·근거 보존

실행이 성공하거나 중간에 실패하면 하네스가 자신이 시작한 시험 노드에 BXDL `stop`을 요청한다. 정상 종료 근거와 `operationBusy=false`를 확인해야 정리된 것으로 기록한다. 정지 확인 실패나 남은 노드가 있으면 전체 결과는 `INCOMPLETE`다. init이 불명으로 끝나면 프로세스가 종료됐다고 추측하지 않으며 node·단계·기록된 PID를 unresolved cleanup으로 남긴다.

CLI 관측 timeout은 실행 취소가 아니다. 지원 코드는 timeout에 프로세스를 강제 kill하지 않고 부분 출력과 PID를 보존한다. 새 init/start, 잠금 삭제, data/WAL 삭제로 성공 상태를 만들지 않는다. 하네스 자체가 중단되었거나 `result.json`이 없으면 종료 성공으로 추측하지 말고 남은 프로세스·서비스와 근거를 확인한다.

`result.json`에는 시나리오별 관측, package/CLI/JAR/Java 식별, 실행 단계, cleanup 결과가 담긴다. `evidence/`에는 CLI JSON·PTY 기록과 합의 관측이 남는다. 출력 폴더 전체에는 시험 개인키·비밀번호·서명된 거래·설정 경로·원문 진단이 있을 수 있으므로 그대로 Git이나 공개 보고서에 넣지 않는다. 공개 결과는 필요한 hash·정제 관측·실패와 미검사 범위를 별도로 추려 작성한다.

하네스의 `PASS`는 이 로컬 package 시나리오와 정상 정리의 성공만 뜻한다. 전체 G1-M, 로그인/로그아웃·sleep/wake, crash/전원 손실, 안전한 제거, 업그레이드, 독립 검토자 재현, 최소 macOS, J5 공급망, Linux/systemd·Docker·LaunchDaemon, 장기 운영과 고객 production 승인은 별도다. 준비나 기동 중 실패한 실행의 `INCOMPLETE`를 나중 실행의 성공으로 덮어쓰지 않는다.

## 도구 자체 검사

이 디렉터리에서 `python3 -B -m unittest -v test_oracles`로 판정·불명 cleanup 회귀를 검사한다. 엔진이나 노드를 시작하지 않는다. Rust 제품 검사는 저장소 루트의 `make check`를 사용한다. 이 두 검사는 위 실제 패키지 인수와 별개다.
