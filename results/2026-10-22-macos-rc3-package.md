# Mac RC.3 패키지 재인수 — 2026-10-22

**PASS — 단일 Mac의 지정 RC.3·Temurin·RocksDB·4-validator 패키지 실행 범위.** [정제 JSON](./2026-10-22-macos-rc3-package.json) · [후보 snapshot](../contracts/nigo/engine-rc3-2026-10-22/README.md) · [재현 도구](../tests/acceptance/README.md) · [설계](../design/2026-10-22-rc3-package-reacceptance.md).

새 RC.3 archive·fixture·DB에서 BXDL 설치·등록·초기화와 사용자 LaunchAgent 기동을 수행했다. 지정한 거래·복구 시나리오와 종료 정리가 통과했고, 독립 검토가 원 거래 증거·설치 파일·관리 사본·시작 단계·OS 종료 상태를 다시 대조했다. 독립 검토는 같은 실행의 증거 감사이며 노드를 별도로 재실행한 시험은 아니다.

## 선택과 구현

- RC.3 전용 engine lock·package spec·수신 snapshot을 채택했다. 공급 JAR는 재빌드하지 않았다. NIGO 저장소는 변경하지 않았다.
- 준비기·입력 guard·실행기의 `--candidate rc1|rc2|rc3` 선택에 RC.3 identity를 추가했다. source/JAR hash·크기·lock·fixture가 섞이면 거부한다.
- RC.3 full 시나리오는 실시간 정족수 복구와 동일 네 DB 전체 재시작 복구를 모두 실행한다. 자동 재제출 없이 재시작 사례에서만 같은 signed transaction을 명시적으로 한 번 재제출한다.
- cleanup의 기존 프로세스 검사와 별도로 시험 포트의 실제 TCP LISTEN 상태를 판정한다. 확인 불가 상태를 PASS로 처리하지 않는다.
- RC.1/RC.2 고정 자료와 기존 동작을 유지했다. 독립 `--scenario same-db-recovery`는 이번에는 실행하지 않았으며 해당 복구 경로를 full 안에서 검사했다.

## 실행 identity

| 항목 | 값 |
| --- | --- |
| 환경 | macOS 26.6.2 (25G83), arm64, 사용자 LaunchAgent |
| BXDL 제품 source | `53bb11b90d9d51aba5a7b509074046fd6ee78401`과 65파일 동일 |
| CLI SHA-256 | `b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef` |
| NIGO 후보 / source | `nigo-engine-v0.1.0-rc.3` / `7ec5abd8c98e1cf070763a16f815e9281b74a5c1` |
| JAR SHA-256 | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` |
| JAR 크기 | 164,088,530bytes |
| runtime | Temurin21.0.12.1+1-LTS full JRE, 259파일·49모듈 |
| archive SHA-256 | `8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017` |
| archive 크기 | 207,160,872bytes |
| manifest SHA-256 | `2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123` |
| package payload inventory | 270파일 / 325,299,323bytes, 외부 Ed25519 시험 key 서명 |

제품은 관리 사본 등록 V2와 시작 단계 진단을 포함한 위 revision으로 빌드했다. 이번 RC.3 채택 과정에서 Rust 제품 source는 바꾸지 않았다. 후보 채택 자료와 인수 도구는 별도 작업 트리 상태이므로 제품 revision을 새 하네스 commit으로 표시하지 않는다. 실행 전 하네스 5파일을 복사·hash 고정했고 종료 후 저장소 및 보존 사본과 다시 대조했다. CLI·lock·공개 검증 key·runtime·하네스 digest는 JSON에 기록했다.

## 실제 package 결과

| 사례 | 결과 |
| --- | --- |
| 수신·선정 JRE 실행 | archive·내장 계약·manifest 확인 후 `engine inspect` 전체 identity 일치 |
| 새 설치·등록·init | 같은 서명 archive의 정상 설치본4개와 별도 wrong-pin 설치본1개; 등록 V2·명시 init 경로 사용 |
| 네트워크 설정 UX | 실제 PTY의 `setup --network`로 native 설정 생성 후 개별 start/status/stop 수행 |
| 4/4 거래 | nonce0, 높이1; 네 노드 receipt·거래 포함·같은 block hash/state root 확인 |
| 3/4 거래·재합류 | nonce1 높이2; 뒤처진 n4가 같은 DB로 재기동해 canonical catch-up, nonce2 높이3 네 노드 확정 |
| 2/4 무정족수 | nonce3, 45.317초·85표본 동안 높이3 유지·미확정, round0→3 |
| 실시간 복구 | n4 복귀 뒤 3/4에서 nonce3 **재제출 없이** 높이4 확정; n3 복귀 후 nonce4 높이5 확정 |
| 별도 round 차이 형성 | nonce5, 45.381초·86표본 동안 높이5 유지·미확정, round0→3 |
| 동일 네 DB 전체 재시작 | round3/3 대0/0에서 모두 정상 종료; 같은 data·키·설정으로 새 attempt 기동; nonce5 같은 bytes 명시1회 재제출 후 높이6 네 노드 확정 |
| 복구 후 새 거래 | nonce6 높이7, 네 노드 receipt·같은 block hash/state root 확인 |
| 잘못된 pin 격리 | outbound WRONG_PIN9건·fault node peer0; 정상 세 노드 nonce7 높이8 확정 |
| 종료 | 정상 service stop12회; cleanup remaining0/errors0; 기록된62 PID 부재·12개 LaunchAgent NOT_LOADED·시험8포트 LISTEN0 |

거래는8개, 제출은9회다. nonce5만 같은 hash로 한 번 명시 재제출했고 nonce3은 재제출하지 않았다. 모든 확정 거래에서 지정 노드의 receipt status/type/hash/height, RPC block 내 거래 포함과 hash/state root, monitor의 같은 높이/hash/root를 독립 대조했다. 두 무정족수 구간의 모든 표본에서 새 거래 receipt가 없고 finalized height가 유지됐으며 UNKNOWN 표본은 없었다. 관측 시간은 SLA가 아니다.

catch-up은 canonical header와 state 일치를 확인했다. 높이2의 n4, 높이4의 n3에서 과거 receipt가 없었던 사실은 `SNAPSHOT_INSTALLS_HEADER_AND_STATE_WITHOUT_HISTORICAL_BODY` 계약에 따라 별도 기록했다. 이 사례를 과거 거래 body 전체 복제의 증거로 보지 않으며, 복귀 후 새 거래의 receipt·거래 포함 판정은 그대로 적용했다.

같은 DB 보존은 data 디렉터리 device/inode, init identity journal·product/native hash와 새 launch attempt를 대조한 결과다. 정상 운용 중 변하는 DB/WAL 전체 bytes 불변을 의미하지 않는다. 엔진 bytes 교체나 DB/round 초기화 없이 복구했으며 강제 kill은 사용하지 않았다.

## 관리 사본·시작 진단·검사

정상4개와 wrong-pin1개 설치본 모두에서 V2 binding과 `control/trust`의 archive·공개 검증 key·engine lock이 원본과 일치했다. trust 디렉터리0700·파일0600, 전체 package membership 및 payload270파일의 size·mode·hash, JRE259파일과 설치된 CLI/JAR를 종료 후 재확인했다.

총12개 시작 attempt가 다음7단계를 순서대로 남겼다. 이 기록은 시작 진행 위치를 보여 주며, 그 자체로 글로벌 합의를 판정하지 않는다.

`INPUT_PINS → PACKAGE_VERIFICATION → COLD_PREFLIGHT → NATIVE_CONFIGURATION → SNAPSHOT_VERIFICATION → FINAL_RECHECK → ENGINE_EXEC`

`make check`의 Rust fmt·Clippy·363 tests와 Python 회귀50 tests, Mac release build가 통과했다. Python 회귀에는 RC.3 후보 혼합 거부, 복구 gate, 거래 증거·중복 제출·DB 보존 판정, LISTEN 검사 실패/불명 처리와 기존 후보 동작을 포함한다. 실제 네트워크 실행 결과는 이러한 자동 회귀와 별도다.

원 결과·하네스·관측 자료·서명 archive·공개 검증 key 등을154파일, 208,355,475bytes로 비공개 작업 폴더에 보존하고 membership·hash·size·mode를 대조했다. 보존 inventory SHA-256은 `fb6dd4cac340f4d1f086d7126d7ebcf87291695ef2930a0b628134e317237fdd`다. 원 결과·독립 감사·빌드 identity·검사 log의 hash는 JSON에 연결했다. 시험 개인키·signed transaction 원문·DB·archive·미정제 로그는 Git에 넣지 않는다.

## 별도 결과와 남은 범위

- 같은 archive의 **원본 공급 자료 부재 후 setup·init·start·재시작13사례 PASS**는 [별도 setup 결과](./2026-10-22-macos-rc3-setup.md)에 기록한다. 이 full 실행에서는 공급 자료를 제거하지 않았다. 전체14문답·cancel/EOF/터미널 hangup을 RC.3에서 모두 재실행한 결과도 아니다.
- **기존 RC.2 DB → RC.3 제품 upgrade는 미구현·미시험**이다. 이번에는 새 RC.3 DB를 만들고 같은 DB로 재시작했다. 기존 binding/lock을 수정하거나 기존 data를 새 설치로 등록하지 않았다.
- 한 호스트·개발용 ZERO fee·시험용 키·mTLS/validator 범위다. 준비에 공급 시험 generator와 Node/ethers를 사용했으며 고객용 PKI 전체나 완전한 외부 네트워크 차단을 검증하지 않았다.
- A7·전체 Native kit 기능·전체 G1-M, 실제 로그아웃과 sleep/wake, crash/power-cut·안전 제거, 다중 호스트·Linux/systemd·Docker·LaunchDaemon·장기/성능 인수는 남아 있다. 로컬 READY/globalConsensus NOT_CHECKED와 이 문서의 거래 증거에 의한 합의 확인은 구분한다.
- J5 공급망·JRE SBOM 불일치·고객 재배포 조건과 과거 NIGO 비용 gate 실패 원인의 UNRESOLVED 상태는 별도다. 정식 고객 출시나 REQ-0003 전체 완료를 선언하지 않는다.
- [수신 시점 receipt](../contracts/nigo/engine-rc3-2026-10-22/consumer-receipt.json)의 package/A7 `NOT_RUN`은 당시 이력으로 보존한다. [RC.1 이전 INCOMPLETE](./2026-10-20-macos-qbft-package.md), [RC.2 package PASS](./2026-10-21-macos-rc2-package.md), [RC.2 setup의 원본 INCOMPLETE](./2026-10-21-macos-rc2-setup.md)와 각 snapshot·lock·spec도 변경하지 않는다.
