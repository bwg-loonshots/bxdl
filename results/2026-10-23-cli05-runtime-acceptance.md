# CLI05-03 — 독립 관리 CLI와 기존 RC.3의 Mac 서비스·네트워크 재인수

- 날짜: 2026-10-23
- 판정: **지정 Mac 조합 범위 PASS. 초기화 보고서의 완전한 진단 수집은 미완료다.**
- 결과: [정제 JSON](./2026-10-23-cli05-runtime-acceptance.json) · [구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md) · [재현 도구](../tests/acceptance/README.md#cli05-03-독립-관리-cli와-기존-rc3-조합)

`cli install`로 설치한 독립 관리 CLI를 통해 기존 RC.3 archive를 그대로 설치·운용했다. 단일 노드 설치·서비스 16개 사례와 4-validator 네트워크 시나리오가 통과했다. 노드 실행 중 관리 CLI를 세 번 전환했고 실행 시도·PID·DB identity·초기화 기록·고정 worker가 유지됐다. 원본 archive·실행기·NIGO JAR·JRE를 다시 만들거나 교체하지 않았다.

이번 변경은 재실행 가능한 서비스/네트워크 인수 하네스, 오프라인 회귀, 결과와 현황 문서다. Rust 제품 코드와 NIGO 저장소는 변경하지 않았다. 원 결과와 별도 검토자의 읽기 전용 사후 감사를 보존한다. 사후 감사는 같은 실행의 증거·파일·OS 상태 재대조이며 노드를 두 번 실행한 결과가 아니다.

## 실행한 CLI와 runtime

환경은 macOS 26.6.2 arm64, 로그인된 사용자의 LaunchAgent다. 두 CLI는 clean Git 소스에서 `tools/build-management-cli.py`로 빌드했고 version은 모두 `0.1.0-dev`다.

| 관리 CLI | 실제 Git commit | 바이너리 SHA-256 |
| --- | --- | --- |
| A · PR #18 최종 head | `994c48f9f872e1ebfbb9acd91d9ae673ef637bbf` | `c9a35e7a5e747210687443efa8a5ed3fa981f8d4b3281c999f80ff56457d9527` |
| B · PR #18 merge | `3a0bb5caa08096454fcb6c88b3186e9a3be02a19` | `99bb09583a9615548d23eb5d088027ccc2831ba5cc0c0f3dacac388209bf2fb8` |

두 commit의 source tree는 `e0ad51db0c68bd4375c334a73d9041d0a5d94fc0`으로 같다. 실제 다른 Git revision을 사용했지만 구현 변경 전후를 비교한 시험은 아니다. **현재 구현과 RC.3의 호환성, 설치된 CLI의 선택·업데이트·복귀 경로**를 확인했으며 임의의 향후 CLI가 자동으로 호환됨을 뜻하지 않는다.

| 공급물 | SHA-256 |
| --- | --- |
| CLI A archive | `6d4a277b3f3b6509127708876af8957f5f7e85b4bed72a4f953bd3b4616e564d` |
| CLI B archive | `2d3862fd02b3e9fddd8f1354f1ac36aaeb22680ab48fda7838249908b0863939` |
| 기존 RC.3 archive | `8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017` |
| 고정 RC.3 실행기 | `b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef` |
| NIGO RC.3 JAR | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` |
| Temurin Java 실행 파일 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |

지원 profile은 `BXDL_RC3_EXECUTOR_V1`, NIGO source는 `7ec5abd8c98e1cf070763a16f815e9281b74a5c1`, JRE는 Temurin21.0.12.1+1-LTS full JRE다. payload270파일·325,299,323bytes, JRE259파일·49모듈을 유지했다. manifest·공개키·lock·빌드 도구와 모든 공급물 identity는 JSON에 기록한다.

CLI 배포본은 이번 시험용 Ed25519 키로 서명하고 최초 설치의 외부 공개키 pin을 이후 update에 재사용했다. 개인 서명키는 제작 후 제거했다. 고객용 release·Apple code signing·notarization 인수는 아니다. 모든 관리 명령은 설치가 소유한 `<bin-dir>/bxdl` 진입점으로 호출했다. `current`를 따라 resolve한 payload나 따로 복사한 바이너리로 우회하지 않았다. CLI root·관리 home·runtime/control/data는 분리했다.

## 단일 노드 서비스 — 16개 사례 PASS

관리 CLI B를 사용했다. 기존 단일 validator 시험 자료에서 공급 archive와 운용 입력만 읽고 새 private 폴더·control·DB에 설치했다. 이전 DB나 control을 재사용하지 않았다. peers가 없고 자동 합의를 시작하지 않는 설정이므로 READY는 `MANUAL_START_INITIALIZED`이며 글로벌 합의는 `NOT_CHECKED`다.

| 과업 | 결과 |
| --- | --- |
| 설치된 진입점·CLI 선택과 hash 확인 | PASS |
| 순차 `setup new` → `setup list/show` | 설정만 준비, 조회 무쓰기 |
| `setup apply` 거절 → 승인 | 거절 시 노드 미생성, 승인 시 설치·등록·이름 연결; DB 초기화 별도 |
| 적용 결과 조회 → 이름 기반 명시 `init` | 조회 무쓰기, 새 DB 초기화 |
| 이름 기반 `start/status/logs/diagnose/stop` | 로컬 READY·정상 종료, 실제 부분 진단 상태 보존 |
| 같은 DB 재시작 → 정상 종료 | 새 attempt, 같은 data device/inode와 engine identity |
| 정지 상태의 `setup apply` 재호출·조회 | package/control/DB 전체 bytes·metadata snapshot 불변, 조회 무쓰기 |
| 공급·입력·payload·JRE·worker와 종료 정리 | 원본/복사 입력12파일씩 불변, worker2개가 고정 실행기와 일치 |

16개 개별 case ID는 JSON에 기록했다. 조회·재apply의 직전 snapshot은 실행 하네스가 비교했다. 독립 감사는 남아 있는 data identity와 runtime은 재대조했지만 저장되지 않은 당시 직전 snapshot을 재구성하지는 못한다.

### 진단 수집에서 확인한 제한

`logs`는 `INCOMPLETE / LOGS_PARTIAL`, `diagnose`는 `INCOMPLETE / DIAGNOSTIC_PARTIAL`을 반환했다. `initialization.report`가 `UNAVAILABLE / MISSING_UNSAFE_OR_CHANGING_INPUT`이었고 실제 init report mode는0644, 서비스 report2개는0600, 각 부모 디렉터리는0700이었다. private 파일만 허용하는 진단 reader 정책에 맞지 않는 init report가 확인됐다. raw stdout/stderr를 정책상 제외하는 항목과 이 수집 실패는 구분한다.

실행 당시 umask는 기록하지 않았다. 서비스 하네스가 umask를 강제하지 않았고0644가 관측됐다는 점은 ambient022의 영향과 부합하지만, **그것만으로 생성 원인을 확정하지 않는다.** 권한을 사후 변경하거나 기존 RC.3 실행기를 교체하지 않았다. 해당 case의 PASS는 진단 파일 생성과 실제 부분 결과의 정확한 보존을 뜻하며, 모든 진단 자료 수집 성공을 뜻하지 않는다.

실물 실행 당시 하네스 요약은 응답의 `reasonCode` 대신 `code`를 읽어 두 코드 필드가 null이었다. 원본 결과는 그대로 두고 원 stdout을 독립 감사로 대조해 위 실제 코드를 기록했다. 이후 하네스 요약만 `reasonCode`를 사용하도록 수정하고 회귀1개를 추가했다. 제품 동작 변경이나 실물 재실행은 없으며 당시 실행 소스와 사후 수정 소스 hash를 JSON에서 구분한다.

후속 **CLI05-03-DIAG-01은 OPEN**이다. 기록한 umask 조건에서 init report 생성 권한을 재현하고 생성/수집 책임을 확인한 뒤, private reader 보호를 약화시키지 않는 개선안을 정한다. 고정 runtime의 수정이 필요하면 새 identity와 별도 재인수가 필요하다.

## 4-validator 네트워크와 CLI 전환 — PASS

고정 JAR/JRE의 준비기로 새 시험 키·체인/네트워크/validator 제품·거래를 만들었다. 설치·등록·초기화는 새 DB에서 수행했다. 네트워크 설정은 설치된 관리 CLI의 실제 PTY `setup --network`, 이후 서비스 운용은 `nodes add`와 이름 기반 명령을 사용했다. 단일 Mac loopback·mTLS·DEV_QBFT·ZERO fee 환경이다.

| CLI 전환 | 실행 중 노드 | 전환 뒤 확인 |
| --- | --- | --- |
| A → B update | 4개 | nonce0, 네 노드 확정 |
| B → A rollback | 4개 | 이후 n4 정지, nonce1 세 노드 확정 |
| A → B update | 3개 | n4 같은 DB 재합류, nonce2 네 노드 확정 |

세 전환의 직전/직후 attempt·nodeInstanceId·PID·data device/inode·engine identity·초기화 journal membership/hash·product/native/binding·worker를 대조했다. CLI 선택만 바뀌었고 살아 있는 노드를 재시작하지 않았다. 이후 복구 과업에서는 명시적으로 노드를 정지·재시작했다.

| 네트워크 과업 | 결과 |
| --- | --- |
| 4/4 → 3/4 → 같은 DB 재합류 | nonce0/1/2가 높이1/2/3에 확정, canonical catch-up |
| 첫2/4 무정족수 | nonce3 한 번 제출,45.365초 경과·86표본, 높이3 유지·round0→3 |
| 실시간 정족수 복구 | n4 복귀 후 nonce3 **재제출 없이** 높이4 확정; n3 catch-up 후 nonce4 높이5 확정 |
| 별도2/4 round 차이 | nonce5 한 번 제출,45.065초 경과·86표본, 높이5 유지·round0→3 |
| 같은 네 DB 전체 재시작 | round3/3 대0/0에서 전부 정상 종료, 같은 data·설정·초기화 identity로 새 attempt 기동 |
| 재시작 후 확정 | nonce5 같은 bytes 명시1회 재제출 후 높이6 확정, 새 nonce6 높이7 확정 |
| 잘못된 pin 격리 | 원래 동일 signer 노드를 먼저 정상 종료; 별도 fault node outbound WRONG_PIN6건·peer0; 정상3개 nonce7 높이8 확정 |

거래8개·제출9회이며 nonce5만 두 번 제출했다. 모든 새 거래의 지정 노드 receipt status/type/hash/height, RPC block의 거래 포함·hash/state root와 monitor 결과를 대조했다. 독립 감사도8개 거래의 원 proof를 다시 확인했다. 두 무정족수 구간의 경과 시간은 각각45초 이상이고, 이산 표본의 첫/마지막 시각 차이는44.836초/44.539초, 최대 간격은0.543초/0.529초다. 연속 계측이나 성능 SLA로 해석하지 않는다.

catch-up은 header/state의 canonical 일치를 검사했다. n4의 높이2, n3의 높이4 과거 receipt는 없었으며 `SNAPSHOT_INSTALLS_HEADER_AND_STATE_WITHOUT_HISTORICAL_BODY` 계약으로 구분했다. 당시 raw catch-up monitor/block 응답은 별도로 저장하지 않아 독립 감사는 frozen runner의 비교와 저장 요약까지 확인했다. 이후 네 노드의 새 거래 proof는 원 receipt·RPC block·monitor 자료로 다시 확인했다. 과거 거래 body 전체 복제를 입증한 결과는 아니다.

같은 DB 보존은 디렉터리 identity·engine identity·초기화 기록·제품/native 설정과 거래 복구를 의미한다. 운용 중 정상적으로 변경되는 ledger/WAL 전체 bytes의 불변을 요구하지 않는다. DB/WAL 삭제·재초기화·round reset·엔진 교체·강제 kill은 수행하지 않았다.

## runtime 불변과 정리

서비스 설치본1개와 네트워크 설치본5개 모두 전체 payload270파일·JRE259파일의 membership·size·mode·hash를 고정 archive와 대조했다. 서비스 worker2개와 네트워크 worker12개는 기존 RC.3 실행기 bytes였다. network init journal과 원 공급물도 유지됐다.

| 관측 | 단일 서비스 | 네트워크 |
| --- | --- | --- |
| 정상 stop | 2회 | 12회 |
| worker / LaunchAgent NOT_LOADED | 2 / 2 | 12 / 12 |
| 기록된 PID 부재 | 27개 | CLI/Java 합집합106개 |
| TCP LISTEN | 시험2포트0 | 시험8포트0 |
| 해당 시험 root의 잔존 프로세스 | 0 | 0 |

독립 감사가 실제 PID·launchctl·TCP LISTEN을 다시 관측했다. 이는 종료 후 해당 시점의 관측이다. 네트워크 감사의 최초 stop 집계는 파일명에 `stop`이 없는 `before-wrong-pin`을 누락해11회로 잘못 집계했다. 실제 `managerCommands.command == stop`과 run report attempt 집합으로12회를 확인하고 감사 기록에 정정 사유를 남겼다. 원 실행 결과의 실패가 아니며 원 결과는 변경하지 않았다.

원 stdout·PTY·transaction proof·하네스 snapshot·결과·독립 감사·검사 로그는 private 작업 폴더에 보존하고 inventory hash로 연결했다. 시험 키·signed transaction 원문·DB·archive·원 로그·개인 경로/PID 목록은 Git 결과에 포함하지 않는다.

## 자동 검사와 남은 범위

| 검사 | 결과 |
| --- | --- |
| `make check` | fmt·Clippy PASS, Rust590 PASS; Python93개 중91 PASS/2 opt-in SKIP |
| 진단 요약 사후 수정 후 Python 전체 discovery | 94개 중92 PASS/2 opt-in SKIP |
| 새 오프라인 하네스 회귀 | 네트워크25 + 서비스7개, 위 전체 Python 수에 포함 |
| 실제 설치 CLI B의 단일 서비스 | 16사례 PASS, 진단 부분 결과 포함 |
| 실제 설치 CLI A→B→A→B와 기존 RC.3 네트워크 | PASS, 독립 사후 감사 PASS |

이번 수정은 Python 하네스·문서·결과만 포함한다. 사후 metadata-only 수정에는 Python 전체 검사를 다시 실행했으며 Rust/실물 시험을 재실행한 것으로 계산하지 않는다. Linux 실제 검증은 수행하지 않았다.

CLI05-03의 지정 조합 인수는 완료했다. **CLI05-04의 작성자가 아닌 사람의 과업은 NOT_RUN**이며 처음 설치·한 항목 수정·다음 날 재개·새 터미널 운용·실패 진단을 별도로 관찰해야 한다. 그 전에 위 진단 수집 제한을 재현·개선할 항목으로 남긴다.

실제 로그아웃/재로그인·sleep/wake, mutation crash/power-cut, 기존 runtime upgrade/DB migration, offline Storage A7, 다중 호스트·Linux/systemd·Docker·LaunchDaemon, 장기/성능·J5·고객 정식 배포는 이 결과로 완료 처리하지 않는다. 이번 조합 인수에 새 NIGO JAR/API 구현은 필요하지 않았다. NIGO의 CLI 분리 보충 회신 수신 확인과 보고서 권한 문제의 책임 범위 확인은 별도 협업 기록으로 다룬다.
