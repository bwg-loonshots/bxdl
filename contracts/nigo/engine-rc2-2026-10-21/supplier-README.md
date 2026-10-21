# NIGO 엔진 개발 후보 nigo-engine-v0.1.0-rc.2

## 식별과 상태

- source: `67c9f8e5100f6ce0df7fc8271eb9d56a8ee7a36b`, `dirty=false`; release line: `release/engine-0.1`.
- JAR: `nigo-node-0.0.1-SNAPSHOT.jar`, 164085159 bytes, SHA-256 `b1aa84cbacf1057746bc5936308345cfdddb8f3fc78b63377af59da05dc0ce10`.
- 계약 fingerprint: `bac1756a81f2d2ddf67ae094fc770bc67060482acab87da7d48b93d441ab8f88`.
- 공급자가 시험한 동일 bytes의 로컬 인계 묶음입니다. 이 파일 생성 자체는 원격 게시·BXDL 수신 또는 제품 인수가 아닙니다.
- private GitHub prerelease 예상 위치: https://github.com/bwg-loonshots/nigo-protocol/releases/tag/nigo-engine-v0.1.0-rc.2
  실제 게시·다운로드 가능 여부와 권한은 저장소의 RC 인계 원장에서 확인하십시오. 저장소 읽기 권한이 필요합니다.
- `development`, `officialRelease=false`, `PROPOSED`를 유지합니다. RC 번호는 protocol/DB/profile revision이 아닙니다.
- NIGO 자체 라이선스·고객 납품 조건은 **UNDECIDED**, 고객 납품 허가는 **NOT_GRANTED**입니다.
  이 개발 후보의 제공·수신·시험 성공은 라이선스 또는 고객 납품 권한 부여가 아닙니다.

## 검증 및 실행

신뢰한 저장소의 인계 기록과 archive checksum을 먼저 대조한 뒤 새 경로에 압축을 풉니다.
이 digest 디렉터리에서 `shasum -a 256 -c SHA256SUMS`를 실행하고 JAR 크기·hash·manifest를 대조합니다.
bundle의 checksum만으로 공급자 인증을 주장하지 않습니다. 후보 ID/태그·full commit·JAR hash/size·전체
`evidence/engine-info.json`·계약 fingerprint를 BXDL lock에 명시적으로 고정하십시오.

```sh
java -jar /absolute/path/nigo-node-0.0.1-SNAPSHOT.jar engine-info
java -jar /absolute/path/nigo-node-0.0.1-SNAPSHOT.jar preflight --config=/absolute/path/node.json
java -jar /absolute/path/nigo-node-0.0.1-SNAPSHOT.jar init --config=/absolute/path/node.json --report=/absolute/path/init.jsonl --attempt-id=init-001
java -jar /absolute/path/nigo-node-0.0.1-SNAPSHOT.jar run --config=/absolute/path/node.json --report=/absolute/path/run.jsonl --attempt-id=run-001
```

선정 Java 21에서 engine-info의 `AVAILABLE`, commit/dirty와 전체 metadata를 확인합니다.
`preflight`의 정상 정적 검사는 `INCOMPLETE`/exit 3이며 DB/native/network 검증 통과가 아닙니다.
init에는 fresh data와 준비된 부모 디렉터리, run에는 같은 chain/node/backend/data identity를 사용합니다.
모든 경로는 symlink가 없는 canonical 경로를 사용합니다. macOS `/tmp` 대신 `/private/tmp`를 사용하십시오.
report는 data 밖의 새 파일이며 attempt ID와 완전한 마지막 JSONL 행을 확인합니다.
별도 stop 명령은 없습니다. 실행 주체가 소유한 정확한 PID에 SIGTERM을 보내고 실제 종료 및 해당 attempt의
`STOPPED / STORAGE_AND_CONSENSUS_CLOSED`, `storageClosed`·`consensusStopReturned`를 함께 확인합니다.
timeout·보고 유실·SIGKILL은 UNKNOWN이며 자동 init·key/DB 삭제·무조건 재시작으로 바꾸지 않습니다.
resume-init의 제한된 중단 초기화 복구 조건은 동봉 `ENGINE_CONTRACT.md`를 따릅니다.

## BXDL 설정 변경 주의

기존 303e163a 후보의 설정을 그대로 재사용하지 마십시오. chain.json에는
`nigo.protocol.native-assets.registration-mode`가 **필수**이며 정확히 `OPEN` 또는 `ALLOWLISTED`여야 합니다.
누락/null/대소문자 alias는 거부합니다. `allowed-issuers` 누락은 빈 목록이며, `OPEN`과 비어 있지 않은 목록은
함께 사용할 수 없습니다. 일반 DB upgrade/downgrade를 보장하지 않습니다. 이번 RC.1→RC.2 forward 검증은
공급자 소유 새 시험 DB의 동일 절대경로·key·config만 유지한 유한 복구 시험이며 사용자 DB 자동 적용 지시가 아닙니다.
정확한 공개 입력·오류·경로·상태 계약은 동봉 `nigo-java/nigo-node/ENGINE_CONTRACT.md`와
`ENGINE_RUNTIME_CONTRACT.md`, contract.json, health-cases.json이 기준입니다.

## 공급자 검증과 미인수 범위

주 소비 목표는 macOS arm64·Java 21·RocksDB fresh data입니다. 공급자 exact-JAR 시험은 H2/RocksDB 각 1회,
4-validator+observer mTLS의 source 전환·동일 높이 hash/root·동일 DB/key 재시작·manual follower·정상 종료를
포함합니다. source-classpath 회귀와 packaged-JAR 9건은 `evidence/test-summary.json`에서 분리합니다.
집계의 `tests`는 보고된 전체 항목 수이며 `passed`와 `skipped`를 별도로 기록합니다. 명시 opt-in
미실행 2건은 `NOT_RUN_OPT_IN`이며 PASS 수에 포함하지 않습니다.
허용 대상은 아래 별도 제품 matrix·진단 method뿐이며, 그 밖의 skip/fail/error는 공급을 거부합니다.
G6의 Native/EVM 두 invocation과 exact-JAR 9건은 모두 실제 PASS가 필수입니다.
QBFT 전체 source suite와 RDB/RocksDB의 `*Qbft*` 선택 회귀는 별도로 집계합니다. 전체 저장소 suite PASS를 주장하지 않습니다.
선정 full JRE의 별도 9개 exact-JAR 사례는 `evidence/external-jre-summary.json`, 새 RC.2 JAR의 live/skew recovery와
RC.1 정체 후 same-DB forward recovery는 `evidence/recovery-summary.json` 및 독립 qualification을 확인하십시오.
기존 사전 후보 `7ce5…/979bc…`의 성공을 이번 최종 source/JAR 성공으로 전용하지 않았습니다.

### 과거 두 FAIL과 새 실행 계약의 검증

과거 source `8d509609a0bd8b12b9cad390625778ac333e9e96`의 canonical attempt 1과 2는 모두 **FAIL**입니다.
RDB ZERO의 p95 비율은 각각 `15.365783826719937` (PARALLEL_1/WAVE),
`13.30035906189949` (PARALLEL_1/READY_QUEUE)이며 원래 상한10을 넘었습니다.
사이에 수행한 단독 비용4-case PASS는 진단일 뿐 공급 인수가 아닙니다.
실패 행의 원시 표본은 남지 않았고, 과거 두 비용 실패의 직접 원인은 **UNRESOLVED**로 보존합니다.

이번 source `67c9f8e5100f6ce0df7fc8271eb9d56a8ee7a36b`는 별도 fresh JVM의 필수 `testUnifiedAssetCost`와
`verifyUnifiedAssetCost`를 사용합니다. 예열1/실측5/raw wall-time/p95비율 상한10을 유지하며
표본 제외·CPU 대체·강제 GC 없이 정확히4개 사례를 일반 node suite와 중복 없이 집계했습니다.
이 새 clean-source 전체 gate PASS는 과거 원인 해결·제품 누수 수정이나 전체 성능 정상 선언이 아닙니다.
`evidence/supplier-attempt-history.json`은 과거 FAIL/PASS진단/FAIL과 이번 새 계약 PASS를 분리하고
원자료의 hash와 정제한 결과를 연결합니다. 실패 원자료는 보존하며 묶음에 그대로 포함하지 않습니다.

- `org.nigo.node.config.StandardProductPersistentNodeTests.publicSupplyAndDefaultProductSurviveColdNodeRestartsAcrossBothBackendsAndFees()` — `NOT_RUN_OPT_IN`; NIGO_S3_NODE_RESTART=1: Separate opt-in Node/SDK cold-restart product matrix is outside this supplier RC scope.
- `org.nigo.protocol.blockchain.pool.PendingStateStoreViewAttributionTests.attributePinnedConstructionWithoutWeakeningItsSnapshot(String)` — `NOT_RUN_OPT_IN`; nigo.u4.pendingAttribution=true: Separate opt-in pending-view attribution/performance diagnostic remains deferred.

원본 manifest의 NOT_RECORDED/NOT_CHECKED는 그대로 두고 실제 시험은 별도 supplier evidence와 digest에 연결합니다.
미정제 XML/log, 환경변수, DB/WAL, 키/password/PKI는 포함하지 않았습니다.

이 묶음은 JRE·BXDL CLI·설치기·source-free QBFT generator를 포함하지 않습니다. 시험 harness가 생성한 설정/키를
고객에게 공급한 것이 아니며, BXDL의 chain/membership·설정/PKI·거래 fixture 준비와 G1-M 실행 계획은 별도입니다.
BXDL이 선정한 exact package/JRE·LaunchAgent 설치/초기화/정상 종료/재시작/합의 인수는 아직 완료되지 않았습니다.
Native P2/P3 후발 모듈·Type-2 개발자 예제·전체 carrier/recipe 인수, Linux/systemd, 성능 MAIN·장기 운영·power-cut,
모든 native 공급망 검증과 고객 납품 승인은 이 결과에 포함되지 않습니다.

기존 후보 PASS를 이 JAR에 전용하지 않습니다. 태그/asset/bytes를 덮어쓰거나 재빌드 JAR로 교체하지 않습니다.
소스·JAR bytes 변경은 새 RC와 새 검증을 요구합니다. 공급 결과 문서 commit과 JAR source commit은 별개입니다.
