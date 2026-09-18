# macOS LaunchAgent 시작·관측·종료

- 작성: 2026-09-18
- 상태: 등록·명시 초기화 이후의 **구현 설계**. 이 문서는 실제 LaunchAgent 실행·G1-M 인수 결과가 아니다.
- 선행: [인스턴스 등록·초기화](./2026-09-18-instance-initialization.md), [제품/native cold 검사](./2026-09-18-product-engine-preflight.md)
- 공급 기준: [clean 개발 후보](../contracts/nigo/development-clean-2026-09-18/README.md)의 source `303e163a9b3f293fa39e42d02b8daa1843973c14`, dirty=false. PROPOSED/development/officialRelease=false와 REQ-0002 OPEN을 유지한다. 공유 NIGO checkout·요구 원장은 변경하지 않는다.

## 1. 이번 범위와 profile

이미 등록·초기화한 Mac arm64 인스턴스를 사용자 LaunchAgent로 시작하고, 실제 runtime 관측과 종료 근거를 확인한 뒤 같은 데이터로 다시 시작하는 흐름을 연결한다. 기존 setup 초안·파일 설치·등록·cold·init/resume-init은 유지한다. 서비스 시작을 위해 새 genesis를 만들거나 초기화 상태를 자동 수리하지 않는다.

첫 profile은 현재 사용자의 GUI 로그인 세션이다. start가 명시 요청될 때만 control의 시도별 plist를 `launchctl bootstrap`으로 등록한다. `~/Library/LaunchAgents`에 plist를 설치하지 않으며 자동 로그인 시작을 제공하지 않는다. `RunAtLoad=true`는 이 수동 bootstrap의 한 번 실행을 위한 값이고, `KeepAlive=false`로 자동 재시작을 사용하지 않는다.

장기 서버 운용용 LaunchDaemon은 후속의 별도 profile이다. 부팅 시 동작, 별도 사용자/권한, 로그인 세션 독립성, 시스템 종료 정책을 별도로 설계·인수해야 한다. 이번 LaunchAgent 결과를 LaunchDaemon·Linux/systemd·Docker 지원으로 확대하지 않는다. 새 daemon·원격 관리 서버·웹 UI도 추가하지 않는다.

## 2. 사용자 명령

```text
bxdl start --instance <absolute-control-dir>
    [--timeout-seconds <1..600>] [--json]
bxdl status --instance <absolute-control-dir>
    [--timeout-seconds <1..120>] [--json]
bxdl stop --instance <absolute-control-dir>
    [--timeout-seconds <1..600>] [--json]
```

기본 timeout은 start 120초, status 10초, stop 60초다. start timeout은 사전 검증·bootstrap 뒤 시작 관측의 예산이며 전체 명령 총 시간 제한이 아니다. status는 조회 시작부터, stop은 stop-request 준비 뒤 관측·정리에서 각 호출의 남은 공통 시간 예산을 사용한다. `--instance`는 제품 instanceId가 아닌 등록된 절대 제어 경로다. 원본 archive·외부 신뢰 key/개발 정책·engine lock·제품/native/chain·credential 참조는 등록 때 고정한 그대로 필요하다.

`service-run`은 LaunchAgent가 시도별 worker를 호출하는 내부 진입점이다. 일반 사용자에게 임의 Java 옵션·plist 편집·shell 실행을 받는 명령이 아니다. 프로그램과 인자는 고정된 배열로 구성하고 shell 문자열 연결을 사용하지 않는다. 내부 명령의 존재가 등록·초기화·시작 허가 검사를 우회할 수 있게 하지 않는다.

start의 대기 시간 만료는 실행 취소가 아니다. status의 조회 성공은 노드 정상 판정이 아니다. stop의 요청 성공은 정상 종료 증명이 아니다. 엔진이 내려가 있거나 관측이 일부 실패해도 가능한 제품·서비스 근거를 반환하고 확인할 수 없는 항목은 UNKNOWN으로 유지한다. 결과의 정확한 envelope/reasonCode는 구현 CLI 계약과 함께 관리한다.

## 3. 제어 자료와 상태

각 start는 새로운 attempt, 고유 label, worker snapshot, plist, engine report를 사용한다. 경로는 등록 control 아래에 두고 원본 입력·data와 분리한다. 고유 label은 다른 control의 같은 instanceId와 충돌하지 않게 구성하며, 대상 domain은 현재 사용자의 GUI domain으로 한정한다.

시도에는 등록 binding, 원본 worker의 identity/hash, 실행 worker snapshot, plist/ProgramArguments, engine/JRE/package identity, native 입력, attempt와 PID의 결속을 보존한다. 이전 시도의 label·report·PID를 다음 실행에 재사용하지 않는다. 원문 설정·credential·child stdout/stderr를 공개 status/error에 반사하지 않는다.

| 저장 단계 | 의미와 허용 범위 |
| --- | --- |
| START_PREPARED | CLI가 시작 intent와 필요한 시도 자료를 내구성 있게 게시함. 실제 bootstrap·gate·Java 실행의 증거는 아님 |
| GATE_CONSUMED(pid) | worker가 해당 시도의 한 번 실행 허가를 소비하고 자신의 PID를 기록함. Java exec 성공·RUNNING·readiness의 증거는 아님 |
| GATE_REFUSED(pid) | 소비한 시도의 검증 또는 exec가 실패함. 자료를 보존하며 같은 attempt 재실행·자동 복구를 허용하지 않음 |
| STOPPED_VERIFIED | 같은 시도의 정상 종료 보고·소유 프로세스 부재·작업 lock 획득을 모두 확인해 게시함. 다음 명시 start의 선행 조건 |

오류·중단·결과 불명은 보존하며 기록 부재나 lock 해제를 성공으로 승격하지 않는다. 첫 실행은 초기화 완료와 runtime 시도 부재를 요구하고, 재시작은 이전 시도의 STOPPED_VERIFIED를 요구한다. START_PREPARED/GATE_CONSUMED 또는 불명 상태만 남은 인스턴스에서 새 start를 자동 허용하지 않는다. 초기화 journal과 runtime 작업 기록은 역할을 구분한다.

CLI와 Java는 같은 UID다. private mode·hash·inode·advisory lock은 실수와 일반 동시 실행을 막는 조건이며 같은 UID 공격자에 대한 강한 격리·암호학적 감사 원장이 아니다. 현재 control 이동/복사/복원·수동 결과 채택 기능도 확대하지 않는다.

## 4. start와 내부 gate

1. start CLI가 control의 변경 lock을 획득한다. 등록/초기화 identity와 이전 runtime 상태, 실행 대상 및 시도 경로를 검사한다. 현재 CLI는 설치 package의 `bin/bxdl`과 같은 바이트여야 한다.
2. 새 worker snapshot·plist·관련 pin을 만들고 START_PREPARED를 내구성 있게 게시한다. 게시 여부가 불명하면 bootstrap하지 않고 자료를 보존한다.
3. CLI가 lock을 해제한 뒤 해당 plist를 수동 bootstrap한다. launchd가 만든 worker는 CLI의 직접 child가 아니므로 CLI의 flock FD가 bootstrap을 통해 전달된다고 가정하지 않는다.
4. worker가 같은 control lock을 별도로 획득한다. 정확한 attempt·START_PREPARED 상태·실행 중인 worker snapshot 경로/hash, launchd job의 자기 PID를 확인한다. 다른 worker가 lock을 보유하거나 소유권이 불명하면 실행하지 않는다.
5. 허가를 GATE_CONSUMED(pid)로 먼저 내구성 있게 소비한다. 그 뒤 등록 입력 pin, 원본 archive의 신뢰, **설치된 전체 manifest inventory**, engine lock/identity와 제품/native 결합 cold를 다시 검증한다. 동일 chain/node/backend/data의 INITIALIZED journal·genesis·기존 ledger를 확인하며 data 존재만으로 초기화 완료를 판단하지 않는다.
6. 원본 입력·설치본과 시도 snapshot을 다시 확인한 뒤 지정 Java로 exec한다. 소비 기록이 실패·불명이면 실행하지 않는다. 소비 뒤 검증·exec가 실패하면 GATE_REFUSED를 기록하며 같은 attempt를 나중에 직접 kickstart로 재사용하지 않는다. 실패 기록 자체가 불명이어도 소비된 시도를 성공으로 바꾸지 않는다.
7. Java는 worker와 같은 PID로 실행된다. 동일 open file description의 lock을 fd 0으로 넘기고 명시 unlock을 하지 않아 Java 종료까지 유지한다. 환경은 정해진 최소 값만 전달하고 ambient JVM 옵션을 제거한다.

Java에는 snapshot JAR, 명시 native config, 새 report 경로, attempt ID를 전달한다. report 경로는 data 밖의 새 파일이다. NIGO `run`이 초기화된 같은 데이터의 canonical identity·독점 접근을 검증하며, 누락/오타 경로를 fresh DB로 만드는 fallback은 허용하지 않는다.

Bootstrap timeout/오류는 job 미생성 또는 worker 미실행의 증거가 아니다. CLI가 불명 상태를 다루는 동안 이미 진전한 worker 기록을 오래된 START_PREPARED로 덮지 않는다. 같은 attempt의 bootstrap·gate·Java 실행을 자동 재시도하지 않는다. 이미 소비된 시도의 직접 `kickstart`는 worker에서 거부한다.

Lock 상속은 선택한 Java/NIGO가 fd 0을 닫거나 바꾸지 않는 실행 조건에 의존한다. engine/JRE 변경 시 그 조건을 다시 인수한다. 초기화에서 확인한 상속 근거를 장시간 run의 검증 결과로 소급하지 않는다.

## 5. status와 runtime 관측

status는 기록·report·launchd와 제한된 loopback HTTP를 읽고 상태를 바꾸지 않는다. 저장된 초기화 상태, 서비스 등록/프로세스 관측, 이번 실행의 report, 로컬 readiness를 구분한다. pid 존재·HTTP 200·head 정체만으로 건강·실패·전역 quorum을 판정하지 않는다. 이전 시도의 결과, 잘못된 identity, 미완성 report, API 단절은 현재 정상 결과로 사용하지 않는다.

Private state와 process report는 크기/구조/field·enum·순서·identity를 엄격히 검증한다. JSONL은 같은 attempt·command·PID와 일관된 sequence를 요구하고 부분 마지막 행을 확정 상태로 취급하지 않는다. `RUNNING.details.nodeInstanceId`와 선택한 build identity를 HTTP 관측에 연결한다.

HTTP 관측은 **bootstrap → health → bootstrap** 순서로 수행해 양쪽 bootstrap의 build/nodeInstanceId와 report를 대조한다. loopback의 명시 IP/port만 사용하며 응답 크기·시간·허용 필드를 제한한다. 두 bootstrap이 일치해도 중간 health가 원자적으로 같은 시점의 snapshot이라는 증명은 아니다. 따라서 관측의 시간·출처와 한계를 보존하며 peer 연결·caught-up·finality·저장소 전체 건강으로 확대하지 않는다.

Bootstrap은 ledger 읽기를 포함하므로 memory-only health와 독립적으로 지연·실패할 수 있다. health 응답만 남았다고 identity 결속을 생략하지 않는다. `READY/MANUAL_START_INITIALIZED`와 `running=false`, observer의 IDLE/NOT_APPLICABLE, sync 비활성의 UNKNOWN 등 공급 계약의 의미를 보존한다. 제품이 현재 validator 입력만 제공하는 것과 runtime DTO의 역할 구분은 별개다.

엔진이 내려간 경우 HTTP를 성공으로 꾸미지 않고 저장 기록과 확인 가능한 서비스 상태를 반환한다. status는 START_PREPARED/GATE_CONSUMED를 소비하거나 STOPPED_VERIFIED를 게시하는 복구 명령이 아니다. `instance show`의 단순 저장 상태·busy 관측도 계속 별도로 제공한다.

## 6. stop과 다음 start의 조건

Java가 실행 중이면 control lock은 Java가 유지하므로 stop CLI가 이를 먼저 독점 획득해 기다리는 구조를 사용하지 않는다. 현재 시도에 결속된 immutable stop-request를 private 경로에 게시하고, launchd의 정확한 domain/label·PID·알려진 ProgramArguments를 관측해 소유 대상임을 확인한 뒤 해당 label에 TERM을 요청한다. PID만으로 임의 프로세스를 종료하지 않는다.

Stop-request에는 대상 시도를 고정하며 **파일 존재를 TERM 전달 증거로 취급하지 않는다.** 요청 게시 뒤 TERM 전에 CLI가 죽을 수도 있고, TERM 뒤 종료 증명 게시 전에 죽을 수도 있다. 후속 명시 stop은 남은 요청과 실제 대상·report를 다시 대조해야 한다. 오래된 요청으로 새 PID/시도에 신호를 보내지 않는다.

다음 조건을 모두 확인해야 종료가 확정된다.

- 정확한 attempt/command/PID의 완전한 terminal report가 `STOPPED / STORAGE_AND_CONSENSUS_CLOSED`다.
- `details.storageClosed=true`, `details.consensusStopReturned=true`이며 report/state identity가 일치한다.
- launchd에서 해당 job의 살아 있는 PID가 없음을 확인했다. 조회 실패·권한/GUI domain 부재를 pid 부재로 대체하지 않는다.
- control의 변경 lock을 실제 획득하고 그 상태에서 시도·report·관련 파일 identity를 다시 확인했다.

위 근거를 확인하면 먼저 STOPPED_VERIFIED를 내구성 있게 게시한다. 이후 job에 살아 있는 PID가 없음을 다시 확인하고 남은 등록을 bootout한다. 기록 게시가 실패·불명이면 bootout하지 않는다. bootout만 실패하면 정상 종료 기록은 보존하며 명시 stop으로 정리를 다시 확인할 수 있다. 다음 start 역시 이전 job이 비활성인지 확인하고 등록을 정리한 뒤에만 새 시도를 준비한다. bootout 호출 자체, signal exit code, lock free 중 어느 하나만으로 정상 close를 선언하지 않는다.

Stop timeout에는 SIGKILL·bootout을 사용하지 않는다. 자료·job·report를 보존하고 UNKNOWN으로 다룬다. 정상 종료를 확인하지 못한 상태에서 release 교체·새 start·init·repair를 자동 호출하지 않는다. FAILED/UNKNOWN 보고를 반복 TERM으로 성공 보고로 바꾸려 하지 않는다.

## 7. launchd와 OS 종료의 차이

시도별 plist의 `ExitTimeOut`은 60초로 둔다. 이 값과 CLI의 stop 관측 timeout은 서로 다른 정책이다. BXDL의 수동 stop이 강제 종료하지 않아도 로그아웃·시스템 종료 등에서 launchd/OS가 자체 종료 절차를 수행하거나 강제 종료할 수 있다. 이를 방지했다고 주장하지 않으며 그 경우 terminal report가 없으면 UNKNOWN이다.

GUI 세션 부재, 로그아웃/로그인, sleep/wake, OS 종료 중 escalation, 부분 bootstrap·등록 누락을 선택한 OS/JRE에서 따로 인수한다. 사용자 세션이 없는 장기 운영의 해결책으로 이번 LaunchAgent를 제시하지 않는다. 자동 로그인 시작·LaunchDaemon은 후속 profile에서 명시적으로 결정한다.

## 8. 검증 계획과 남은 범위

실제 실행 결과는 별도 results 기록에 남긴다. 이 문서에는 테스트 개수·PASS·실행 성과를 선기록하지 않는다. 기존 [초기화 결과](../results/2026-09-18-instance-initialization.md)는 run/launchd/정상 stop/restart의 인수 결과가 아니다.

필요한 실패 시험은 동일 instance 중복 start, bootstrap 전후 CLI 중단, gate 검사/consume/exec 경계 실패, 직접 kickstart, stop-request 게시와 TERM 사이 중단, 종료 보고 이후 기록 게시 중단, 다른 label/PID·stale/partial report·HTTP identity 불일치, stop timeout·잔존 process다. 각각 불명 보존과 타 인스턴스 무영향을 확인한다.

실제 smoke는 새 시험 전용 register/init 후 start → 관측 → 정상 stop → 같은 데이터 restart → stop 순서로 수행한다. 원본 시험 control/data를 복사해 identity 검사를 우회하지 않는다. 같은 JAR/JRE를 재사용하더라도 새 CLI를 기존 설치본에 덮어쓰지 않고, 최종 인수에는 그 CLI를 담은 새 archive/설치본을 사용한다. 소스 checkout/Gradle 없이 공급 JAR를 실행하며 사용자/NIGO 실제 노드를 시험 대상으로 삼지 않는다.

Peer 없는 manual-start 단일 validator smoke는 서비스 수명주기 확인이며 4-validator 거래·handshake·동기화·G1-M 전체 인수가 아니다. setup 연결, logs/diagnose·제거, 장기 운영 profile, 정식 JRE/최소 OS 선정, UNKNOWN 수동 조정·이동/복원·등록 migration·attempt GC는 별도 잔여 범위로 유지한다.
