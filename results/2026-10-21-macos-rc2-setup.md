# Mac RC.2 전체 설치 UX 재검증 — 2026-10-21

**Run B의 설치 UX 20개 사례 PASS / 원본 자동 집계 INCOMPLETE / 후속 독립 정리 확인 PASS.** 원자료 접근 경로 문제는 OPEN이다. 전체 결과를 무조건 PASS로 집계하지 않는다. [정제 JSON](./2026-10-21-macos-rc2-setup.json) · [설치 도우미](../docs/setup.md) · [앞선 4-validator 패키지 인수](./2026-10-21-macos-rc2-package.md).

요청한 실제 Mac 설치 문답과 로컬 서비스 흐름의 검증을 수행했다. 두 실행의 원본 결과는 모두 `INCOMPLETE`로 보존하며, 사례별 성공·시작 실패·추가 정리 관측을 아래와 같이 구분한다. 이번 작업에서 BXDL 제품 코드를 바꾸거나 패키지를 새로 만들지 않았다.

## 니고 정식 회신과 실행 기준

먼저 NIGO 전용 worktree에서 RC.2 패키지 인수 회신을 작성하고 [PR #225](https://github.com/bwg-loonshots/nigo-protocol/pull/225)를 머지했다. 머지 commit은 `684ba347872b16a424feaec1122f2e0b1c7dc38b`다. 공유 NIGO checkout은 수정하지 않았다. 회신의 전체 설치 UX `NOT_RUN`은 회신 작성 시점의 상태이며, 이 문서가 그 뒤 수행한 검증 기록이다.

| 항목 | 고정한 값 |
| --- | --- |
| 환경 | macOS 26.6.2 (25G83), arm64, 사용자 GUI 세션 |
| BXDL checkout | `ca66fc4c0eecfefabb88f5dca0e3a33abfbedea5` |
| Rust 제품 source | `Cargo.toml`·`Cargo.lock`·`src/**/*.rs` 63개가 `2955ba5cdedeaf6dba65a98a905588e71770abea`와 bytes 일치 |
| CLI SHA-256 | `120089de764575709fe5d708a2d502df7dafd0b659ec4086c1176bfb8c0a803d` |
| NIGO 후보 / source | `nigo-engine-v0.1.0-rc.2` / `67c9f8e5100f6ce0df7fc8271eb9d56a8ee7a36b` |
| JAR SHA-256 | `b1aa84cbacf1057746bc5936308345cfdddb8f3fc78b63377af59da05dc0ce10` |
| runtime | Temurin 21.0.12.1+1-LTS full JRE, 259파일 |
| archive SHA-256 | `45b74e9145996a90d33f354972f07adf23cc4bc2d34881c45eab30bc9ba694ce` |
| archive / 설치 inventory | archive 207,123,432bytes / 설치 파일 270개·325,229,584bytes |
| 서명 | 앞선 인수와 동일한 외부 Ed25519 시험 key 서명; 정식 배포 서명 아님 |

기존 archive·외부 공개키·engine lock과 CLI bytes를 다시 대조했다. 새 제품 빌드, JAR 교체, archive 재서명은 없었다. 각 실행은 새 fixture·시험 키·설치 폴더·DB를 사용했다. A의 등록이나 DB를 고쳐 B로 이어가지 않았다.

`setup --network` 실제 PTY에서 새 native 설정을 준비했다. 이번 로컬 설치 흐름용으로 consensus auto-start와 sync를 명시적으로 끄고, 준비 단계에서 peer 속성 21개를 제거해 peer 0개로 고정했다. 이후 제품/native·chain·키 참조 hash를 고정했다. 이는 합의를 자동 시작하는 운영 설정이나 4-validator 재시험이 아니며, 실행 중 `globalConsensus=NOT_CHECKED`를 유지한다.

## Run A — 시작 전 대기와 원본 실패 보존

A는 `Documents` 작업 폴더에 보존한 archive·공개키·lock을 참조했다. 문답·수정·취소/EOF·재개·설치·cold·명시 init·start 거절까지 13개 사례가 PASS였다. 실제 cold 검사와 초기화의 JVM 실행은 성공했다.

명시 start 뒤 LaunchAgent worker는 `GATE_CONSUMED`에 머물렀다. 준비 상태 대기는 `SERVICE_STATUS_WAIT_EXPIRED`, 엔진·readiness는 `UNKNOWN`이었다. worker sample은 `__open`에서의 대기를 보였지만, 어느 원자료 접근이 대기를 일으켰는지와 macOS 보호 폴더 권한(TCC)의 관련성은 확정하지 못했다. `Documents` 경로 의존은 조사 가설로 남긴다.

명시 stop 요청 뒤 worker는 `SERVICE_START_CANCELLED`로 종료했다. 해당 runtime run의 report는 없었고, stop 결과는 `FAILED/GATE_REFUSED`, job은 `EXITED`, operation busy는 false였다. 앞서 cold/init JVM이 실행된 사실과 시작 과정의 cold JVM 가능성을 구분하며, 이 결과를 정상 실행 엔진의 종료 검증으로 취급하지 않는다.

원시 하네스의 `cleanup.outcome=PASS`는 당시 확인한 PID·포트만 반영하고 `failureStop=UNRESOLVED`를 반영하지 못한 집계 한계다. 원본을 수정하지 않고 별도 기록으로 바로잡았다. 독립 확인에서 worker 부재와 소유 job의 `NOT_RUNNING_EXIT_3`를 확인한 뒤 종료된 job만 bootout했고, `NOT_LOADED` 및 시험 포트 반환을 확인했다. **A의 실행 결과는 INCOMPLETE, 물리적 잔존 자원 정리는 PASS, 정상 engine stop 검증은 false**다.

## Run B — 전체 문답과 로컬 lifecycle

A의 결과를 보존한 후 새 fixture와 새 출력 경로를 사용했다. 동일 archive·공개키·lock을 byte 대조해 별도 시험 공급 폴더에 복사했다. B의 worker가 `Documents`의 공급 자료를 참조하지 않도록 설치 전에 선택한 경로이며, 기존 등록의 binding을 수정한 것은 아니다. 임시 시험 경로를 장기 운영용으로 권장하지 않는다.

하네스는 실패한 명령의 JSON에서도 worker PID를 수집하고, 실패 정지가 미확정이면 cleanup을 `INCOMPLETE`로 유지하도록 보완한 뒤 실행 전에 다시 고정했다. 이 보완은 제품 코드 변경이 아니다. A와 B는 각각 한 번 수행했으며 실패 명령의 자동 재시도는 없다.

| # | 실제 PTY·서비스 사례 | 관측 결과 |
| --- | --- | --- |
| 1 | 동일 서명 archive와 새 로컬 profile 고정 | package identity·서명·새 입력 확인 PASS |
| 2 | archive 입력 뒤 `:cancel` | exit 5, 입력 보존, 설치·등록·data 미생성 |
| 3 | `:help`·잘못된 이름·`:back` 수정·문답 EOF | 잘못된 값 재입력, 3개 답 보존, exit 5 |
| 4 | 5개 답 저장 뒤 TTY 접속 종료 | PTY master close로 controller SIGHUP 종료, checkpoint 보존 |
| 5 | 재개 후 전체 14문답·자동 계획 | RC.2 source와 정책 표시, 계획 revision 1 |
| 6 | `edit`로 14문답 재검토·명시 `plan` | Enter로 기존 답 유지, 이전 계획 무효화, revision 2 |
| 7 | `apply` 확인 거절 | 제품 출력·설치·등록·data 미생성 |
| 8 | `apply` 확인에서 EOF | exit 5, 출력 미생성, 명시 재개 |
| 9 | `apply` 확인에서 `:cancel` | exit 5, 출력 미생성 |
| 10 | 명시 `apply` 승인 | 설치·등록, 14개 답과 생성 제품 일치, cold INCOMPLETE·binding MATCHED |
| 11 | `init` 거절 후 `finish` | data·추가 attempt 미생성, 재개 시 자동 init 없음 |
| 12 | 별도 `init` 승인 | INITIALIZED, genesis 일치 |
| 13 | `start` 거절 후 `finish` | SERVICE_NOT_STARTED·NOT_REGISTERED·NOT_CHECKED, 추가 attempt 없음 |
| 14 | 별도 `start` 승인 후 setup `wait` | 로컬 READY, 같은 attempt 관측, 새 시작 없음 |
| 15 | READY 후 대기 중인 setup의 TTY 접속 종료 | controller SIGHUP 종료, 서비스는 같은 PID·attemptId·nodeInstanceId로 READY 유지 |
| 16 | 실행 중 setup 재개·`check`·`finish` | 기존 실행 관측, 새 attempt 없음 |
| 17 | 첫 정상 `stop` | SERVICE_STOPPED_VERIFIED·NOT_LOADED·STOPPED·busy=false |
| 18 | 개별 `start`로 같은 DB 재시작·setup `wait` | 새 attemptId/nodeInstanceId, 같은 data device/inode·genesis·engine identity |
| 19 | 두 번째 정상 `stop` | SERVICE_STOPPED_VERIFIED·NOT_LOADED·STOPPED·busy=false |
| 20 | 정지 후 최종 setup 재개·`finish` | STOPPED 관측, 수정·재설치·init/start 메뉴 없음, 새 attempt 없음 |

20개 사례 모두 PASS다. 초기화는 1회, 서비스 실행은 2회이며 별도 동의를 하지 않은 작업이 재개 때문에 실행되지 않았다. 설치 이후 `edit`·`inputs`·`plan`을 다시 제공하지 않는 것도 확인했다. 작업 폴더·checkpoint의 0700/0600 권한, 고정 입력 15개, 생성 제품과 등록 binding, 실행 하네스 2개 파일의 불변을 대조했다.

TTY 종료는 실제 controlling PTY의 master를 닫고 controller가 SIGHUP으로 종료되는 것으로 검증했다. 중단은 문답 또는 메뉴 대기 중에만 수행했다. Mac 계정 로그아웃, 터미널 앱 전체 종료, apply/init 중간 강제 종료를 검증한 것으로 확대하지 않는다.

## B의 자동 집계와 독립 종료 확인

B는 위 사례와 runtime 259개 파일 검증까지 마쳤지만 마지막 `bind()` 확인에서 HTTP 28590을 다시 확보하지 못해 원본 `outcome=INCOMPLETE`로 끝났다. 사례 실패 사유는 없으며, 원본의 `remainingListeners`는 bind 실패를 적은 필드명이다. 해당 실패만으로 LISTEN 소켓 존재를 확정할 수 없다.

후속 독립 관측은 같은 실행의 종료 상태를 읽기 전용으로 확인했다. 추가 start나 전체 재실행은 하지 않았다.

- 두 서비스 attempt 모두 LaunchAgent `NOT_LOADED`, 기록 PID 26개 전부 부재.
- HTTP 28590·P2P 29590의 `lsof` LISTEN 행 0개, `netstat` 일치 행 0개. 후속 bind도 두 포트 모두 성공.
- 정상 stop 2회·동일 DB 재시작·TTY 종료 후 같은 PID를 원시 결과와 교차 확인.
- 설치 inventory 전체 270파일·325,229,584bytes, runtime 259파일의 membership·size·mode·SHA-256 재검증 PASS.

따라서 **설치 UX 사례는 PASS, 최종 정리의 독립 확인도 PASS**다. 원본 자동 집계 `INCOMPLETE`는 그대로 유지한다. 최초 bind 실패 순간의 원인은 미확정이며 TIME_WAIT 때문이라고 단정하지 않는다. B 성공으로 A의 접근 경로 원인이나 경로 UX 문제가 해결된 것으로 취급하지 않는다.

## 증거 보존

원본 실행은 `bxdl-rc2-setup-20261021-a`·`bxdl-rc2-setup-20261021-b`이며 각기 다른 fixture·data를 사용했다. 작업 폴더 `work/bxdl-rc2-setup-acceptance/preserved-run-a`에 28파일·142,024bytes, `preserved-run-b`에 40파일·161,604bytes의 원 결과·PTY·관측 JSON·실행 당시 하네스를 보존했다. 별도 독립 정리·감사 JSON과 보존 inventory를 [정제 JSON](./2026-10-21-macos-rc2-setup.json)의 hash로 연결한다.

B의 하네스 SHA-256은 `770bdc6a962add99e5f7b94dd30583373e882814bebba58b1f90a6db696b4574`다. 실제 실행 뒤 이 하네스와 원본 결과를 바꾸지 않았다. 개인키·비밀번호·DB·archive·미정제 실행 자료는 Git에 넣지 않는다.

## 후속 작업과 판정 한계

- **원자료 경로와 서비스 접근 UX:** 보호 폴더 접근의 정확한 실패 원인 조사, 영구 보관 위치 설계, 서비스 관점의 접근 사전 진단과 시작 실패 안내가 필요하다. 현재 CLI에는 해당 보호 경로 사전 진단이 없다.
- **인수 도구의 종료 관측:** bind 가능 여부와 OS listener 존재를 구분하는 oracle을 후속 개선한다. A의 실패 정지 집계 한계와 B의 bind 관측 한계는 원본 이력으로 남긴다.
- **전체 G1-M:** apply/init 진행 중 crash·전원 손실·실패 주입, 로그아웃·sleep/wake, 안전한 제거, 독립 운영자 반복 인수는 남아 있다. 이번 20개 사례 완료가 전체 G1-M 완료는 아니다.
- **나머지 제품 범위:** 기존 RC.1→RC.2 upgrade, 공급망/J5·고객 재배포, 다중 호스트·Linux/systemd·Docker·장기 운용 인수는 후속이다. 네트워크 합의 근거는 앞선 4-validator 보고서를 별도로 따른다.
