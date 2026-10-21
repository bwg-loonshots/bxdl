# Mac RC.2 패키지 재인수 — 2026-10-21

**PASS — 단일 Mac의 지정 RC.2·Temurin·RocksDB·4-validator 패키지 실행 범위.** [정제 JSON](./2026-10-21-macos-rc2-package.json) · [후보 snapshot](../contracts/nigo/engine-rc2-2026-10-21/README.md) · [재현 도구](../tests/acceptance/README.md) · [설계](../design/2026-10-21-rc2-package-reacceptance.md).

RC.1의 [이전 INCOMPLETE 기록](./2026-10-20-macos-qbft-package.md)은 보존한다. 이번 결과는 새 RC.2 archive·새 fixture·새 DB에서 처음부터 실행한 인수이며, 이전 실패를 이어서 PASS로 바꾼 것이 아니다. NIGO 공급자 시험 대신 BXDL이 설치·등록·초기화하고 사용자 LaunchAgent로 기동한 실제 package를 검사했다.

## 선택과 구현

- RC.2 전용 engine lock·package spec·수신 snapshot을 추가했다. 공급 JAR는 재빌드하지 않았다.
- 준비기·입력 guard·실행기는 `--candidate rc1|rc2`를 명시 선택한다. source/JAR hash·크기·lock·fixture가 다른 후보와 섞이면 거부한다.
- RC.2 full 시나리오는 실시간 정족수 복구와 별도의 동일 네 DB 전체 재시작 복구를 모두 수행한다. 자동 재제출은 금지하고 재시작 사례만 원 signed transaction의 한 번 명시 재제출을 허용한다.
- 기존 RC.1 고정 진단·실패 자료는 보존했다. 독립 `--scenario same-db-recovery`도 제공하지만 이번에는 full만 실행했다.
- 오프라인 인수 도구 회귀 28개를 `make test-acceptance`로 제공하고 `make check`에 포함했다.

## 실행 identity

| 항목 | 값 |
| --- | --- |
| 환경 | macOS 26.6.2 (25G83), arm64, 한 사용자 GUI 세션 |
| BXDL Rust 제품 source | `2955ba5cdedeaf6dba65a98a905588e71770abea`와 동일 |
| CLI SHA-256 | `120089de764575709fe5d708a2d502df7dafd0b659ec4086c1176bfb8c0a803d` |
| NIGO 후보 / source | `nigo-engine-v0.1.0-rc.2` / `67c9f8e5100f6ce0df7fc8271eb9d56a8ee7a36b` |
| JAR SHA-256 | `b1aa84cbacf1057746bc5936308345cfdddb8f3fc78b63377af59da05dc0ce10` |
| runtime | Temurin21.0.12.1+1-LTS full JRE, 259파일·49모듈 |
| archive SHA-256 | `45b74e9145996a90d33f354972f07adf23cc4bc2d34881c45eab30bc9ba694ce` |
| manifest SHA-256 | `abf486cc8c6b093dd88eb5d77f95b09111e1711838692931fbe85c3316fa3d32` |
| package inventory | 270파일 / 325,229,584bytes, 외부 Ed25519 시험 key 서명 |

Rust 제품 source는 변경하지 않았고 CLI bytes도 기반 commit과 같다. RC.2 채택 자료와 새 인수 도구는 실행 당시 작업 트리 상태이며 40자리 제품 revision을 새 하네스 commit이라고 주장하지 않는다. 실행 전에 하네스 5개 파일을 복사·hash 고정했고 종료 후 현재 파일 및 보존 사본이 일치했다. 세부 digest는 JSON에 기록했다.

## 실제 package 결과

| 사례 | 결과 |
| --- | --- |
| 수신·선정 JRE 실행 | archive·내장 계약·manifest 확인 후 BXDL `engine inspect`의 전체 identity MATCHED |
| 새 설치·등록·init | 동일 서명 archive의 정상 설치본4개, 정상 genesis 일치; 별도 wrong-pin 설치본1개 |
| 네트워크 설정 UX | 실제 PTY의 `setup --network`로 각 native 설정 생성, 기존 제품 등록·명시 init/start/stop 경로 사용 |
| 4/4 거래 | nonce0, 높이1; 네 노드 receipt·거래 포함·동일 hash/root 확인 |
| 3/4 거래·재합류 | nonce1, 높이2; 같은 DB로 뒤처진 노드 재시작·canonical catch-up; nonce2 높이3 네 노드 확정 |
| 2/4 무정족수 | nonce3, 45.038초·84표본 동안 높이3 유지·미확정, round0→3 |
| 실시간 복구 | 3/4 복귀 뒤 nonce3 **재제출 없이** 높이4 확정; 네 노드 복귀 후 nonce4 높이5 확정 |
| 독립 skew 형성 | nonce5, 별도45.243초·85표본 동안 높이5 유지·미확정, round0→3 |
| 동일 네 DB 전체 재시작 | round 차이3/3 대0/0에서 모두 정상 종료; 같은 data·키·설정으로 새 attempt 기동; nonce5 동일 bytes 명시1회 재제출 후 높이6 네 노드 확정 |
| 복구 후 새 거래 | nonce6 높이7, 네 노드 receipt·동일 hash/root 확인 |
| 잘못된 pin 격리 | outbound CREDENTIAL_PIN/WRONG_PIN3건·peer0; 정상 세 노드 nonce7 높이8 확정 |
| 종료 | 정상 service stop12회; cleanup remaining0/errors0; 기록된62 PID 부재·시험8포트 listener0 |

두 무정족수 관측 구간에서 UNKNOWN 표본은 없었다. 같은 DB 보존은 data 디렉터리 device/inode, init identity journal·product/native hash와 새 attempt/nodeInstanceId로 대조했다. 정상 실행 중 바뀌는 DB/WAL 전체의 물리 bytes 불변을 요구하지 않는다. 재시작 뒤 명시 재제출 직전 네 노드는 peer3·원 거래 receipt 없음이며 round는3/3/0/0이었다.

거래8개에 제출은9회다. nonce5만 같은 hash를 명시적으로 한 번 재제출했고 nonce3은 재제출하지 않았다. 모든 새 거래에서 지정 노드의 receipt status/type/hash/height, RPC block 내 거래 포함과 block hash/state root, monitor의 같은 높이/hash/root를 독립 대조했다. snapshot catch-up의 과거 receipt는 별도 존재 여부로 남기며 새 거래 판정은 완화하지 않았다.

종료 후 정상4개와 부정1개 설치본 모두의 CLI/JAR, JRE259파일 membership·size·mode·hash를 다시 확인했다. PID·포트 확인은 정상 stop 보고와 별도 OS 관측이다. 강제 kill이나 data/WAL/round 삭제는 사용하지 않았다.

## 검사와 근거 보존

최종 `make check`에서 Rust fmt·Clippy·343 tests와 Python 회귀28 tests가 통과했고, Mac release build도 통과했다. Python 회귀는 잘못된 후보/lock/크기, 거래 누락·변경, 자동 중복 제출, 불명 정지·DB 교체 때의 중단과 거짓 finality 성공을 검사한다. 실제 네트워크 결과는 단위 시험과 별도다.

실행 root는 `/private/tmp/bxdl-rc2-acceptance-20261021-a`, fixture는 별도 `/private/tmp/bxdl-rc2-fixture-20261021-a`다. 원 결과·하네스·관측 자료·서명 archive/공개 검증 key의150파일을 작업 폴더 `work/bxdl-rc2-acceptance/preserved-run-a`에 별도로 보존했다. 보존 inventory, 원 결과, 독립 감사, 검사 log hash를 정제 JSON에 연결했다. 시험 개인키·signed transaction 원문·DB·archive·미정제 로그는 Git에 넣지 않는다.

## 남은 범위

- **RC.1 기존 설치 → RC.2 제품 upgrade는 미구현·미시험**이다. 이번에는 처음부터 RC.2로 만든 DB를 재시작했다. 기존 binding/lock을 고치거나 비어 있지 않은 data를 새 등록하지 않았다.
- 이번에는 네트워크 설정 PTY와 개별 install/register/init/start/status/stop을 사용했다. 전체14문답·취소/재개의 `setup --install` TTY 흐름을 RC.2로 재실행한 결과가 아니다.
- 한 호스트·개발용 ZERO fee·시험용 키·mTLS/validator 범위다. 준비 단계는 공급 JAR의 시험 generator와 Node/ethers로 자료를 만들었다. 고객용 PKI 발급/설정 도구 전체 또는 완전한 외부 네트워크 차단을 검증하지 않았다.
- 전체 G1-M, 로그인/로그아웃·sleep/wake, crash/power-cut·안전 제거, 다중 호스트·Linux/systemd·Docker·장기/성능 및 Native kit 전체 기능 인수는 남아 있다. 관측 시간은 SLA가 아니다.
- J5 공급망·JRE SBOM 불일치·고객 재배포 조건은 별도다. 과거 NIGO 비용 gate 실패 원인의 UNRESOLVED 상태도 유지한다. 이 PASS로 정식 고객 출시나 REQ-0003 전체 완료를 선언하지 않는다.
