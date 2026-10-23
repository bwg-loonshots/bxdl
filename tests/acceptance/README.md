# BXDL Mac 4-validator 패키지 인수 도구

실제 RC.2 full 실행은 [2026-10-21 결과](../../results/2026-10-21-macos-rc2-package.md)를, 기존 RC.3 패키지 실행은 [2026-10-22 결과](../../results/2026-10-22-macos-rc3-package.md)를 따른다. 독립 관리 CLI의 설치 진입점과 같은 고정 RC.3를 조합한 후속 [CLI05-03 인수 결과](../../results/2026-10-23-cli05-runtime-acceptance.md)는 아래 별도 runner를 사용했다. 도구 지원과 실행별 실제 판정은 구분한다.

이 도구는 새 시험 자료와 격리된 로컬 인스턴스로 BXDL 패키지를 검증한다. 고객 설치기나 고객용 키·CA 발급기가 아니다. 이 문서는 실행 절차와 판정 범위를 설명하며, 현재 시험의 PASS를 선언하지 않는다.

첫 대상은 로그인된 GUI 세션이 있는 macOS arm64, RocksDB, 사용자 LaunchAgent다. HTTP와 P2P를 모두 `127.0.0.1`에 두고 노드별 포트를 분리한다. 같은 사용자 UID의 네 인스턴스이며, 서로 다른 호스트나 강한 보안 격리를 검증하는 구성이 아니다.

## 도구와 실행 경계

| 파일 | 역할 | 실행하지 않는 것 |
| --- | --- | --- |
| `prepare_fixture.py` | 명시 선택한 고정 RC1/RC2/RC3 JAR의 `QbftDevnetGenerator`로 새 시험용 NGVK·개별 mTLS·신뢰 자료 생성, 공개 목록과 제품 설정 작성, 새 거래 지갑과 미리 서명한 Type-2 거래 준비 | BXDL 설치·등록·초기화·노드 기동·거래 제출 |
| `run_acceptance.py` | 새 시험 서명키로 실제 archive 조립·검증, 같은 archive의 네 설치본 준비, 네트워크 설정 도우미·등록·초기화·LaunchAgent·거래/합의 관측 | 고객 인스턴스 인수, 기존 설치본 덮어쓰기, 운영 키 발급 |
| `candidates.py` | RC1/RC2/RC3의 source·JAR hash/크기·거래 개수 고정 | 움직이는 branch HEAD나 재빌드 JAR 채택 |
| `test_candidates.py` | 후보/lock 혼합, 거래 입력 변경, 자동 재시도, 동일 DB 복구의 거짓 성공 방지 회귀 | 실제 엔진·서비스 실행 |
| `test_oracles.py` | 잘못된 receipt/block의 거짓 성공, historical receipt 제한, init 불명 cleanup의 오프라인 회귀 | 실제 엔진·서비스 실행 |
| `fixture_guard.py` | 선택 후보 시험 fixture의 identity·loopback·파일 범위·입력 hash와 legacy B 재개 allowlist 검사 | credential 내용 읽기·암호학적 거래 재검증 |
| `support.py` | 새 파일 저장·복사, 실제 PTY 문답, CLI JSON 수집, 제한된 상태 검증과 비공개 근거 기록 | 독자적인 복구·서비스 제어 정책, import 시 실행 |
| `run_pin_only.py` | 보존한 서명 archive와 새 fixture에서 잘못된 pin 거부·정상 3-node 합의를 독립 확인 | 앞선 catch-up·정족수 복구 실패의 성공 전환 |
| `run_recovery_probe.py` | 보존한 B/C 실행의 정지 근거를 확인하고, 같은 DB에서 거래 명시 재제출과 합의 라운드 관측 | 범용 복구·DB 초기화·자동 재전송 |

준비 단계에도 고정 JAR의 Java 도구는 실행한다. 그러나 노드를 시작하거나 노드 DB를 초기화하지 않는다. 준비 결과의 `PREPARED_NOT_EXECUTED`는 자료 준비만 뜻한다. 빈 `data` 상위 폴더는 만들지만 노드별 data는 생성하지 않는다.

Node와 ethers는 준비 단계의 오프라인 거래 서명에만 필요하다. 실행 단계는 준비된 거래 bytes를 읽고 설치된 CLI/JRE/JAR를 사용한다. NIGO checkout·Gradle·Node/ethers를 노드 실행 중 호출하지 않는다. 다만 인터넷 차단과 시스템 호출을 계측하는 하네스는 아니므로, 이를 네트워크 차단 상태의 완전한 source-free 인수 결과로 표현하지 않는다.

## 고정 공급물과 사전 조건

`prepare_fixture.py`와 `run_acceptance.py`는 `--candidate rc1|rc2|rc3`를 필수로 받는다. 후보를 파일명·움직이는 branch HEAD에서 추정하지 않는다. RC1의 기존 이력·lock·legacy 진단 allowlist는 보존하며, 서로 다른 후보의 fixture/lock과 섞어 실행할 수 없다. hash가 다른 JAR/Java를 최신 버전이라는 이유로 대신 넣지 않는다.

| 대상 | 고정 값 |
| --- | --- |
| RC1 NIGO source | `540a89f0814c01d2a4ec67da1087af35195882ca` |
| RC1 JAR SHA-256 | `f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45` |
| RC1 JAR 크기 | `164083923` bytes |
| RC2 NIGO source | `67c9f8e5100f6ce0df7fc8271eb9d56a8ee7a36b` |
| RC2 JAR SHA-256 | `b1aa84cbacf1057746bc5936308345cfdddb8f3fc78b63377af59da05dc0ce10` |
| RC2 JAR 크기 | `164085159` bytes |
| RC3 NIGO source | `7ec5abd8c98e1cf070763a16f815e9281b74a5c1` |
| RC3 JAR SHA-256 | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` |
| RC3 JAR 크기 | `164088530` bytes |
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
  --candidate rc3 \
  --java /path/to/verified-payload/runtime/bin/java \
  --jar /path/to/verified-payload/engine/nigo-node.jar \
  --node /path/to/node \
  --ethers-module /path/to/node_modules/ethers/lib.esm/index.js \
  --output /path/to/new-fixture \
  --http-base-port 28080 \
  --p2p-base-port 29090
```

준비기는 새 validator 네 개의 시험 자료, `chain.json`, `network.json`, `products/n1.json`부터 `n4.json`, 새 지갑, RC2/RC3에서는 nonce 0~11, RC1에서는 기존대로 nonce 0~7의 미리 서명한 거래를 만든다. 자료는 새 private 폴더에 보관한다. 생성된 `application.yml`은 공급자 생성기 근거로 남기며, 실행 설정으로 자동 채택하지 않는다.

고정 keytool이 있으면 공개 leaf 인증서를 내보내 NIGO domain hash와 manifest pin을 대조한다. 없으면 `certificateExport=NOT_AVAILABLE`로 기록한다. 실패한 준비 폴더는 덮어쓰거나 재사용하지 않고 보존한다. 성공 여부와 생성 범위는 `fixture-summary.json`으로 확인한다.

시험 원장에는 새 거래 지갑만을 명시적으로 funding한다. 공개 devnet 고정 지갑이나 고객 자산을 사용하지 않는다. `ALLOWLISTED` 등록 정책의 빈 허용목록을 유지하며 이 시험에서 Native 모듈 게시를 수행하지 않는다.

## 2. 실제 패키지 설치·실행

`--run`을 생략하면 실행하지 않는다. 아래 명령은 package 생성·설치·초기화·서비스 시작과 거래 제출을 실제 수행한다.

```sh
python3 ./run_acceptance.py \
  --run \
  --candidate rc3 \
  --scenario full \
  --cli /path/to/release/bxdl \
  --revision FULL_40_HEX_BXDL_COMMIT \
  --fixture /path/to/new-fixture \
  --payload-source /path/to/verified-payload \
  --lock /path/to/engine-lock.rc3-temurin-development.json \
  --spec /path/to/package-spec.rc3-temurin.example.json \
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
4. RC2/RC3에서는 새로운 별도 2/4 구간에서 미확정 거래를 한 번 제출하고 45초 round skew를 형성한다. active 두 노드의 round가 정지한 두 노드의 같은 view height에서 관측한 round보다 실제로 높아야 한다. 네 노드 모두의 정상 종료를 확인한 뒤 같은 설치본·control·DB로 동시에 재시작한다. 새 attempt/nodeInstanceId, data 디렉터리 identity·엔진 초기화 journal·product/native 설정 보존을 확인하고, 원래 signed transaction의 같은 bytes를 정확히 한 번 명시 재제출한다. 네 노드 모두의 성공 receipt/확정과 후속 새 거래의 확정을 요구한다. 이 단계는 DB/WAL 삭제·재등록·재초기화·엔진 JAR 교체를 하지 않는다.
5. node ID가 가장 작은 정상 노드를 먼저 정상 종료한다. 같은 시험 identity를 별도 설치·control·새 data에서 사용하되 세 peer의 expected pin을 모두 틀리게 만든다. 이 부정 노드만 `new-network`로 설정해 sync를 끄고 archive는 켠다. catch-up 대기로 인증 거부 관측이 가려지지 않도록 한 별도 시험이며 정상 네 노드의 all-join 설정은 바꾸지 않는다. 해당 노드의 `OUTBOUND / CREDENTIAL_PIN / WRONG_PIN` audit와 peer 0, 정상 세 노드의 새 거래 확정을 확인한다. 동일 signer의 두 프로세스를 함께 시작하지 않는다.

현재 부정 시험은 잘못된 expected pin에 따른 격리다. 잘못된 role/validator, 만료·폐기 인증서, unknown CA 전체 행렬이나 부정 시험 뒤 원래 네 노드의 full-mesh 복원까지 완료했다고 표시하지 않는다.

거래는 준비 단계에서 서명한 원 bytes를 사용한다. live 2/4→3/4 복구에서는 첫 제출 이후 재제출하지 않는다. RC2/RC3 전체 동일 DB 재시작 시나리오만 원 거래의 두 번째 제출을 명시적으로 허용하고, 세 번째 제출이나 타 nonce의 무단 재시도를 거부한다. 제출 응답 유실은 `UNKNOWN_OBSERVE_ORIGINAL_HASH`로 남기고 원 hash를 조회한다. 하네스가 다시 서명하거나 자동 재전송하지 않는다. 각 제출의 nonce·hash·포트·차수와 명시 재제출 여부를 별도 evidence에 기록한다. legacy 복구 probe의 재제출은 아래에 명시한 진단 단계다. 서로 다른 노드의 finality certificate hash는 quorum seal 구성에 따라 다를 수 있으므로 동일성 조건에 넣지 않는다.

**시나리오 3은 RC1 당시 공급자 시험보다 긴 정족수 부족과 재제출 없는 복구를 관측한다.** 공급자 `04-qbft-multiprocess.spec.ts`는 첫 round 증가를 최대 45초 기다린 뒤 바로 세 번째 노드를 복구하고, 그 노드에 같은 signed transaction을 명시적으로 재제출한다. `QBFT_DEVNET_RUNBOOK.md`는 reconnect 이전 pending backfill을 보장하지 않는다. RC1 당시 기본 실행의 복구 timeout 하나만으로 결함을 확정하지 않았으며, 별도 재시작·재제출 probe와 공급자 재현으로 원인을 구분했다. RC2/RC3의 live 복구와 전체 동일 DB 재시작은 각각 새 관측으로 판정하고, 이전 실패를 새 후보의 성공으로 덮어쓰지 않는다.

| 거래 nonce | RC2/RC3 `full` | RC1 `full` |
| --- | --- | --- |
| 0 | 최초 네 노드의 거래 확정 | 동일 |
| 1 | 네 번째 노드 정지 중 거래와 catch-up 기준 | 동일 |
| 2 | 첫 재합류 후 새 거래 | 동일 |
| 3 | live 2/4 구간 최초 1회 제출, 3/4 복구 후 재제출 없이 확정 | 동일 |
| 4 | live 복구 뒤 네 노드의 새 거래 | 동일 |
| 5 | 별도 2/4 45초 구간 최초 1회 제출, 네 DB 재시작 후 명시 1회 재제출 | wrong-pin 중 정상 세 노드 거래 |
| 6 | 네 DB 재시작 복구 뒤 새 거래 | 미제출 예비 |
| 7 | wrong-pin 중 정상 세 노드 거래 | 미제출 예비 |
| 8~11 | 미제출 예비 | 생성하지 않음 |

### RC2/RC3 동일 DB 복구만 독립 실행

`--candidate rc2|rc3 --scenario same-db-recovery`는 후보 하나를 명시해 새 fixture·출력·control·data를 만드는 별도 실행이다. 최초 네 노드의 nonce 0 확정을 확인하고 nonce 1로 45초 skew·전체 정상 정지·같은 네 DB 재시작·원 거래 1회 재제출을 수행한 뒤 nonce 2 새 거래를 확인한다. 앞선 live 복구 시험이 실패했더라도 그 결과를 보존하고 새 경로에서 이 시나리오를 별도로 수행할 수 있다. `--continue-from`이나 기존 실패 DB 재활용 옵션이 아니다.

RC2/RC3의 `full`과 `same-db-recovery`는 처음부터 선택한 후보로 초기화한 DB의 복구를 시험한다. 이전 후보의 DB를 새 후보 패키지로 이전하거나 기존 설치의 JAR를 교체하는 BXDL 업그레이드 인수가 아니다. `engineUpgradeTested=false`를 기록하며 기존 `rc1ToRc2UpgradeTested=false` 필드도 이력 호환을 위해 유지한다. 새 실행을 반복하면 새 fixture/출력 경로를 준비하고, 앞선 실행의 정상 cleanup과 포트 해제를 확인한다.

전체 재시작 전후에는 data 폴더의 device/inode와 `engine-instance.json`, product/native 파일 hash를 대조한다. 정상 기동 중 변경되는 실제 ledger/WAL 파일 전체의 hash 불변을 요구하지 않는다. 엔진 DB 내용 보존과 복구는 같은 genesis·원 거래·후속 거래의 확정 관측으로 추가 확인한다.

## Snapshot catch-up의 과거 receipt와 제한된 이어 실행

고정 RC1의 `HistoricalFinalizedLedgerInstaller`는 snapshot 이전의 역사 구간을 canonical header·certificate·checkpoint로 설치한다. state는 snapshot으로 가져오며, 그 구간의 transaction body와 과거 ETH receipt를 모두 복원하는 계약은 아니다. 같은 source의 `nigo-java/nigo-storage-rocksdb/STORAGE_LAYOUT.md`에서도 historical header와 body/receipt 가용성을 구분한다.

따라서 `canonical_at`은 대상 높이까지의 finality와 동일 block hash/state root를 요구하고, 과거 receipt 유무를 `historicalReceiptPresent`로 따로 기록한다. 이 경우 `historicalReceiptRequiredByThisCase=false`를 명시한다. 반면 재합류 뒤 실제로 처리하는 새 거래의 검사는 완화하지 않는다. 네 노드 모두의 성공 receipt와 같은 확정 block을 확인해야 한다.

이전 실행은 최초 4-node·3-node 거래 확정 뒤 snapshot catch-up 노드의 과거 ETH receipt를 기다리다 `FINALITY_TIMEOUT_same-db-catchup`으로 `INCOMPLETE`가 되었고 시험 노드들은 정상 종료했다. 이 기록은 유지한다. 과거 receipt까지 요구했던 하네스 조건을 RC1의 지원 범위와 구분하며, 새로운 이어 실행의 성공을 미리 선언하지 않는다.

`--continue-from`은 `--candidate rc1 --scenario full`에서만 이 특정 실행을 이어 검증하는 경로다. 범용 resume·UNKNOWN 복구·결과 수동 채택 기능이 아니다. 다음 조건이 모두 필요하다.

- 이전 `result.json`이 `INCOMPLETE`, reason이 정확히 `FINALITY_TIMEOUT_same-db-catchup`이고 cleanup이 `remaining=[]`, `errors=[]`다.
- 이전 archive와 입력 CLI hash가 이전 결과에 고정된 값과 같고, 이전 서명 archive 검증이 통과한다.
- 기존 네 control의 현재 status도 정상 종료 근거를 만족한다. 남아 있는 실행을 강제로 중지하거나 기록을 고쳐 조건을 만들지 않는다.
- 이전과 같은 fixture·engine lock·CLI·revision을 지정하고, 결과를 저장할 출력 폴더만 새로 준비한다. 수록본은 B의 정확한 result/public/transaction/lock/revision hash allowlist로 이 제한을 검사한다.

```sh
python3 ./run_acceptance.py \
  --run \
  --candidate rc1 \
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

`run_pin_only.py`와 `run_recovery_probe.py`는 2026-10-20의 특정 archive·CLI hash를 코드에 고정한 재현 도구다. 임의 후보를 시험하는 범용 옵션으로 해석하지 않는다. 실제 입력과 필요한 이전 실행이 없으면 새 기본 실행부터 준비한다. 각 도구의 `--help`로 경로 인자를 확인하며 `--run`을 명시해야 실행한다. 두 legacy 도구는 후보 선택 없이 RC1만 취급하며, RC2/RC3 fixture를 넣으면 거부한다. 특히 `run_recovery_probe.py`의 고정 helper hash는 과거 원본을 가리키므로 현재 보완본을 통과시키려고 hash를 재고정하지 않는다. 해당 기록의 정확한 보존 source 묶음을 사용하거나, RC2/RC3 검증에는 위의 새 `--scenario same-db-recovery`를 사용한다.

Pin-only는 새 fixture와 HTTP 24420~24423·P2P 25420~25423을 사용한다. 기존 실행과 다른 identity를 확인하고 같은 archive에서 정상 세 노드와 잘못된 expected pin을 가진 한 노드를 설치한다. 정상 세 노드의 TLS1.3 인증과 거래 확정, 부정 노드의 outbound WRONG_PIN·peer 0을 확인한다. 기존 실행의 control/data는 재사용하지 않는다.

Recovery probe는 완전히 정지한 B/C 실행과 원 fixture A를 검사한다. 네 노드를 함께 재시작하고 이전 미확정 nonce 3의 같은 signed transaction을 한 번 명시 재제출하여 기준 높이를 확인한다. 이것이 실패하면 다음 거래로 넘어가지 않는다. 기준이 확인되면 두 노드를 정상 정지하고 nonce 4를 한 번 제출해 45초 정체를 관측한다. 세 번째 노드 복구 후 그 노드에 동일 bytes를 한 번 재제출하고 최대 90초 동안 canonical finality와 progress를 수집한다. 새 서명·무한 재전송·DB/round 초기화는 수행하지 않는다.

저장소 수록 전 `run_acceptance.py`의 receipt transaction hash/type·block 높이·거래 포함 검사를 강화하고, catch-up에서 존재하는 receipt도 대조하도록 보완했다. finality timeout에는 제한된 progress 근거를 추가했다. fixture·거래·lock 입력 pin, loopback·credential 참조 범위와 legacy B allowlist, init의 불명 결과를 cleanup 미정리로 남기는 처리도 추가했다. **B/C/D/E 실행에 사용한 원본과 이 보완본은 hash가 다르다.** 원 실행의 PASS를 보완본 전체 재실행으로 소급하지 않으며, 실행별 하네스 hash·추가 독립 검사는 [검증 기록](../../results/2026-10-20-macos-qbft-package.md)에 구분한다.

## 실패·종료·근거 보존

실행이 성공하거나 중간에 실패하면 하네스가 자신이 시작한 시험 노드에 BXDL `stop`을 요청한다. 정상 종료 근거와 `operationBusy=false`를 확인해야 정리된 것으로 기록한다. 정지 확인 실패나 남은 노드가 있으면 전체 결과는 `INCOMPLETE`다. 정상 stop 기록과 별도로 `cleanupListeners`에 시험 HTTP/P2P 포트의 실제 OS TCP LISTEN 관측을 기록한다. `lsof`의 명확한 부재만 PASS이며 listener가 있거나 관측이 불명하면 전체 결과는 `INCOMPLETE`다. bind 실패를 listener 존재로 추정하거나 관측 때문에 프로세스·서비스를 조작하지 않는다. init이 불명으로 끝나면 프로세스가 종료됐다고 추측하지 않으며 node·단계·기록된 PID를 unresolved cleanup으로 남긴다.

CLI 관측 timeout은 실행 취소가 아니다. 지원 코드는 timeout에 프로세스를 강제 kill하지 않고 부분 출력과 PID를 보존한다. 새 init/start, 잠금 삭제, data/WAL 삭제로 성공 상태를 만들지 않는다. 하네스 자체가 중단되었거나 `result.json`이 없으면 종료 성공으로 추측하지 말고 남은 프로세스·서비스와 근거를 확인한다.

`result.json`에는 명시 후보·시나리오, 입력 pin, 시나리오별 관측, package/CLI/JAR/Java 식별, 제출 차수, 실행 단계, cleanup 결과가 담긴다. 핵심 하네스 source를 `evidence/harness-source/`에 복사하고 실행 전 hash와 종료 시 불변 여부를 기록한다. 실행 중 하네스가 바뀌면 전체 결과는 `INCOMPLETE`다. `evidence/`에는 CLI JSON·PTY 기록과 합의 관측이 남는다. 출력 폴더 전체에는 시험 개인키·비밀번호·서명된 거래·설정 경로·원문 진단이 있을 수 있으므로 그대로 Git이나 공개 보고서에 넣지 않는다. 공개 결과는 필요한 hash·정제 관측·실패와 미검사 범위를 별도로 추려 작성한다.

하네스의 `PASS`는 이 로컬 package 시나리오와 정상 정리의 성공만 뜻한다. 전체 G1-M, 로그인/로그아웃·sleep/wake, crash/전원 손실, 안전한 제거, 업그레이드, 독립 검토자 재현, 최소 macOS, J5 공급망, Linux/systemd·Docker·LaunchDaemon, 장기 운영과 고객 production 승인은 별도다. 준비나 기동 중 실패한 실행의 `INCOMPLETE`를 나중 실행의 성공으로 덮어쓰지 않는다.

## 도구 자체 검사

이 디렉터리에서 `python3 -B -m unittest -v test_oracles test_candidates test_listeners`로 판정·불명 cleanup 회귀를 검사한다. 엔진이나 노드를 시작하지 않는다. Rust 제품 검사는 저장소 루트의 `make check`를 사용한다. 이 두 검사는 위 실제 패키지 인수와 별개다.


## CLI05-03: 독립 관리 CLI와 기존 RC.3 조합

2026-10-23 실제 결과는 **지정 Mac 서비스·네트워크 범위 PASS**다. [정제 결과](../../results/2026-10-23-cli05-runtime-acceptance.md)에 서비스16사례·네트워크12개 실행 시도·거래8개/제출9회·실행 중 CLI 전환3회와 정확한 binary/runtime identity를 기록한다. A/B는 같은 source tree의 실제 PR head `994c48f`와 merge `3a0bb5c`이며 합성 revision이 아니다. 미래 모든 CLI 또는 정식 고객 배포의 인수를 뜻하지 않는다.

`run_management_service.py`와 `run_management_acceptance.py`는 `cli install`이 만든 **설치 진입점**을 사용한다.
`--manager-entry`에는 `<bin-dir>/bxdl`을 그대로 전달한다. `current`를 resolve한 payload 바이너리나 별도 복사본을
실행하면 CLI 업데이트/복귀 경로를 검증할 수 없다. 두 도구 모두 기존 RC.3 archive·공개키·lock·실행기를 정확한
hash로 확인하며 runtime을 조립하거나 JAR를 재빌드하지 않는다. 기존 package 실행기를 직접 사용하는 위
`run_acceptance.py`와 구분한다.

서비스 인수는 준비한 단일 validator의 `supply`와 `operational inputs`만 읽어 새 private 출력으로 복사하고
경로·instanceId·포트를 바꾼다. 기존 control·DB는 입력으로 채택하지 않는다. `--fixture`는 이 두 폴더를 포함하는
이전 시험 root이고, 대신 `--supply`와 `--inputs`를 명시할 수도 있다.

```sh
python3 -B tests/acceptance/run_management_service.py \
  --run --manager-entry /test/service-bin/bxdl --cli-root /test/service-cli \
  --manager-sha256 EXPECTED_MANAGER_BINARY_SHA256 \
  --fixture /test/prior-single-validator-fixture \
  --output /test/new-service-run --instance-id cli05-service-example \
  --http-port 29680 --p2p-port 30680
```

순차 `setup new`·조회·apply 거절/승인·별도 명시 init·이름 기반 start/status/logs/diagnose/stop·같은 data 재시작과
종료 후 재apply의 불변성을 검사한다. 해당 단일 validator는 peers 없는 수동 시작으로 로컬 준비 상태를 검사하며
네트워크 합의 근거가 아니다. 실패하면 원 결과를 `INCOMPLETE`로 보존하고 자동 재시도·kill·정리하지 않는다.
`serviceMayRemain`과 `lastCommandStage`를 확인하여 시험 소유 control의 상태를 관측하고 필요하면 별도 정상 stop을
수행한다. 이 후속 정리는 실패한 시험을 PASS로 바꾸지 않는다.

서비스 도구는 `logs`/`diagnose`의 완전수집 성공과 partial 반환을 구분한다. 이번 실제 결과는 `INCOMPLETE`·`LOGS_PARTIAL`·`DIAGNOSTIC_PARTIAL`이며 init report mode0644와 private reader 거절을 관측했다. 실행 당시 umask는 기록하지 않아 원인은 미확정이다. 서비스16사례의 PASS는 partial의 정확한 표현을 포함한 인수 판정이며, 진단 완전수집 자체는 **OPEN**이다. 기존 report 권한이나 판정을 바꿔 완전수집 PASS로 만들지 않는다.

네트워크 인수 전에 위 준비기로 **새 RC.3 fixture**를 만들고, 전용 CLI root에는 첫 관리 CLI를 설치한다.
`--bundle-next`는 같은 신뢰키로 검증할 다음 CLI 배포본이며 `--revision`은 그 CLI의 실제 full Git commit이다.
관리 목록은 새 `--output/management-home`만 허용한다.

```sh
python3 -B tests/acceptance/run_management_acceptance.py \
  --run --manager-entry /test/network-bin/bxdl --cli-root /test/network-cli \
  --manager-home /test/new-network-run/management-home \
  --bundle-next /test/next-cli.tar.gz --revision FULL_NEXT_CLI_GIT_COMMIT \
  --archive /test/original-rc3.tar.gz --public-key /trusted/rc3-public.pem \
  --lock /test/engine-lock.rc3.json --fixture /test/new-rc3-fixture \
  --output /test/new-network-run
```

설치·네트워크 설정·등록 뒤 이름으로 운용하며 기존 full의 finality·catch-up·두 복구 경로·wrong-pin oracle을
재사용한다. 기동한 네 노드를 유지한 채 A→B 업데이트, B→A 복귀, 세 노드가 기동한 상태에서 A→B 재전환을
수행한다. 전환 직전/직후의 attempt·PID·data/초기화·설정·worker identity를 비교하고 전환 뒤 실제 거래 확정을
요구한다. 초기화 journal의 파일 목록·hash도 전환 전후와 전체 실행 종료 때 비교한다. 정상 cleanup과 전체 payload/JRE·worker·report에 기록된 Java PID 및 CLI PID·LaunchAgent·실행 root 프로세스·TCP LISTEN 상태도 대조한다. report/프로세스 관측이 누락되거나 불명이면 종료 성공으로 취급하지 않는다.

두 도구의 `result.json`, CLI stdout/PTY 원문, fixture와 정리 관측은 private 출력에 보존한다. 공유 결과에는
버전·hash·시험 범위와 정제한 판정만 남긴다. 실제 인수 실행과 기본 Python 오프라인 회귀를 구분하며,
로그아웃/sleep/crash·엔진/DB upgrade·다중 호스트·Linux·J5·CLI05-04 사람 과업까지 확대하지 않는다.
