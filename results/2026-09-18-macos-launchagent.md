# Mac 사용자 LaunchAgent 구현·실행 검증

- 날짜: 2026-09-18
- 기준: `ae7741d` 이후 `feat/macos-launchagent` 변경. 아래는 로컬 구현·실행 검증 기록이며, 원격 CI 결과는 이 변경의 PR에서 별도로 확인한다. 공식 릴리스 기록은 아니다.
- 대상: 사용자 GUI 세션의 macOS 26.6.2 (25G83), Darwin 25.6.0, arm64.
- 엔진: clean source `303e163a9b3f293fa39e42d02b8daa1843973c14`, PROPOSED/development. NIGO 공유 checkout·요구 원장은 변경하지 않았다.
- 연결: [설계](../design/2026-09-18-macos-launchagent.md), [명령](../docs/cli.md), [인스턴스 사용](../docs/instance.md).

## 구현 범위

`start/status/stop --instance`를 추가했다. start는 검증한 설치 package의 CLI와 같은 bytes를 요구하고, 새 attempt의 worker/JAR/native snapshot과 private plist를 만든다. worker는 launchd의 정확한 job/PID와 자기 snapshot을 확인한 뒤 한 번 실행 허가를 소비한다. 등록 입력·원본 archive·설치 전체 inventory·제품/native cold·INITIALIZED journal·genesis·기존 ledger를 다시 검사하고 Java로 exec한다. worker와 Java는 같은 PID이며 fd 0으로 instance flock을 이어받는다.

수동 bootstrap에만 `RunAtLoad=true`를 사용한다. `KeepAlive=false`이며 자동 재시작, `~/Library/LaunchAgents` 설치, 로그인 자동 시작은 제공하지 않는다. launchctl은 절대 경로와 인자 배열로 호출하고 출력·시간을 제한한다. 조회할 때 프로그램·인자·작업 폴더·로그 경로를 정확하게 대조하며 알 수 없는 출력은 UNKNOWN이다.

status는 같은 attempt의 JSONL/build/PID/nodeInstanceId와 bootstrap → health → bootstrap을 연결한다. 역할·정수 문자열·nullable·열거값을 검사하고 로컬 관측 시각의 5초 허용 범위를 확인한다. health는 독립 표본이며 READY를 전역 quorum·거래 확정·저장소 전체 건강으로 확대하지 않는다.

stop은 해당 attempt의 명시 의도를 남기고 검증한 job label에 TERM을 보낸다. 같은 실행의 STOPPED/closure 증거, launchd 프로세스 부재, control 잠금 획득을 함께 확인한 뒤 종료 기록을 게시하고 정지한 job만 bootout한다. timeout에 SIGKILL이나 live job bootout을 하지 않는다. 이전 종료가 검증되기 전 새 start는 거부한다.

## 자동 검사

| 검사 | 결과 |
| --- | --- |
| `make check` | PASS: fmt, all-targets Clippy `-D warnings`, 총 200 tests |
| `cargo build --locked --release` | PASS: 이 호스트의 arm64 CLI |
| `make check-linux` | PASS: x86_64-unknown-linux-gnu 타입 검사. Linux 실행·systemd 인수 아님 |

200개는 library 178, CLI 10, CLI integration 2, setup CLI 5, setup safety 5다. 새로운 검사는 private service journal의 state/PID/binding·경로·동시 기록, inherited lock, snapshot 변경, 등록 역할과 health 불일치, bounded HTTP framing, strict runtime DTO·공급자 health fixture, launchctl print 식별·부재 구분·XML·파일 권한·호출 deadline을 포함한다.

처음 sandbox 실행에서는 실제 loopback socket 및 기존 특수 파일 테스트가 권한으로 실패했다. 새 state의 optional PID가 JSON null로 저장되어 strict decoder에 거부되는 구현 오류도 확인했다. optional field 생략으로 수정하고 로컬 권한이 허용된 환경에서 최종 200개 전체를 통과했다. 낮은 권한 실행의 실패를 무시하거나 테스트를 생략하지 않았다.

## 실제 패키지·자료

이전 시험의 control/data를 복사하거나 설치된 CLI만 덮지 않았다. 새 CLI를 담은 archive를 만들고 새 경로에 install → register → init을 수행했다. 기존 clean JAR와 시험용 JRE 및 공개 chain/시험 credential만 새 시험 입력으로 사용했다. 설치 후에는 그 package의 `bin/bxdl`과 동봉 Java/JAR를 실행했으며 NIGO 소스·Gradle은 호출하지 않았다.

| 산출물 | SHA-256 |
| --- | --- |
| 최종 시험 archive, 181,212,558 bytes | `3df2363f5314697827e9336457c301514b055d1067d7cb469b0ab6fcdfb93078` |
| 시험한 CLI, 2,276,288 bytes | `984cd58930ddd893b4487c1e91fd8f691b97ad1e02b9b4eae3f85243e4a91cc3` |
| NIGO JAR, 159,988,739 bytes | `dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455` |
| Java launcher, 69,936 bytes | `94b66f2cc8edfba9e0e8b25abe93e4c3617d2cc1af7e8eaec0f5b2afe0df5c12` |

CLI는 현재 release 빌드와 시험 package 안의 bytes가 같음을 별도로 비교했다. `Cargo.toml`, `Cargo.lock`, `src/**/*.rs`를 repository-relative 경로 구성요소 순으로 정렬하고(Python `sorted(Path 목록)`), POSIX 경로를 사용해 `path + NUL + raw bytes + NUL`로 계산한 구현 파일 digest는 `c78142156fcd8ad14547f6556038c563599a9c065c3be8ba00af38d1327b99b8`이다. 이 로컬 digest는 서명된 공급 attestation이 아니다.

JRE는 이전 초기화 시험의 Oracle 21.0.7 jlink 시험물이며 `java.se,jdk.crypto.ec,jdk.unsupported,jdk.management`를 포함한다. 정식 runtime vendor·재배포 선정·지원 최소 OS가 확정된 것은 아니다. package는 명시 opt-in한 unsigned development 시험물이며 고객 배포물이 아니다.

실제 구성은 단일 QBFT VALIDATOR, RocksDB, HTTP/P2P loopback, peer 없음, sync/consensus auto-start 비활성이다. 시험용 chain/NGVK/TLS 자료와 별도 포트를 사용했다. 운영자·공유 NIGO의 실제 node/data/key는 사용하거나 종료하지 않았다.

## 실제 실행 결과

| 사례 | 결과와 근거 |
| --- | --- |
| 첫 시작과 상태 | `RUNNING`, `READY / MANUAL_START_INITIALIZED`. launchd PID와 report·bootstrap identity를 결속 |
| 중복 start | `INSTANCE_BUSY`로 거부. 실행 중 Java의 inherited control lock 유지 |
| 정상 stop | `SERVICE_STOPPED_VERIFIED`. closure 두 값·정확한 attempt/PID·job 종료·lock 획득 확인 후 등록 해제 |
| 같은 데이터 재시작 | 기존 DB·키·경로를 그대로 사용하고 새 attempt로 READY. `nodeInstanceId`는 변경, INITIALIZED journal/genesis는 동일 |
| start CLI 실제 중단 | LaunchAgent worker가 GATE_CONSUMED를 게시한 뒤 **직접 소유한 start CLI만 SIGKILL**. worker/Java는 계속 진행해 READY, 다른 start는 잠금으로 거부, 명시 stop 정상 완료 |
| 직접 kickstart | 해당 시험 job에 TERM 후 정상 종료를 관측하되 등록은 유지. 같은 job을 kickstart하면 `SERVICE_GATE_REPLAY_REJECTED`, 데이터 파일 전체 hash 불변. 이어서 명시 stop으로 등록 정리 |
| 기본 위치·공백 경로 | `Library/Application Support/BXDL` 아래 고유 시험 control/data/config에서 새 register/init/start/status/stop 성공. 같은 최종 package를 사용 |
| 시험 정리 | 정상 시험에서 생성한 총 6개 job 모두 정확한 label로 조회해 미등록 확인. 시험 엔진을 실행 중으로 남기지 않음 |

첫 탐색 시험은 Documents 아래에서 job을 등록했지만 worker 기록·엔진 report 전에 launchd exit `78: EX_CONFIG`로 종료됐다. 자료를 보존했고 이를 정상 시작·정상 엔진 종료로 승격하지 않았다. 이후 모든 package/control/입력 자료를 독립 `/private/tmp` 경로로 준비한 시험은 통과했다. 보호 폴더/TCC가 원인이라는 단정은 하지 않는다. 실패한 첫 job은 시험 정리 과정에서 PID 없음·exit78·엔진 report 없음·lock 해제를 확인한 뒤 그 label만 해제했다. 제품 UNKNOWN 자동 복구 기능을 구현한 것은 아니다.

실제 macOS의 `last exit code = 78: EX_CONFIG` 형식을 adapter 회귀 fixture에 반영했다. 알려진 sysexits 코드/기호의 정확한 조합만 수용하며, 불일치나 임의 문자열을 숫자 상태로 채택하지 않는다.

## 남은 인수·제품 범위

- 전체 오프라인 구간의 egress 차단, 4-validator 거래/finality·mTLS peer 부정 사례, full G1-M은 수행하지 않았다. 이번 manual-start READY는 합의 참여 증거가 아니다.
- 로그인/로그아웃, sleep/wake, OS shutdown/escalation, GUI 세션 부재의 실제 환경 인수와 추가 OS·설치 환경에서의 배포 UX 검증이 남아 있다. BXDL이 SIGKILL하지 않아도 OS는 강제 종료할 수 있다.
- 실제 power-cut, 장시간 장애·부하, stop CLI의 모든 crash 지점, arbitrary DB 복구를 검증하지 않았다.
- gate 거부·불명 시도의 수동 조정, 설정/키 변경·등록 migration, 인스턴스 이동/복원, attempt/JAR/CLI/private 로그 보존·용량 관리는 후속이다.
- setup 전체 연결, logs/diagnose/uninstall, LaunchDaemon, Linux/systemd, Docker, 정식 공급/서명·JRE 선정과 릴리스 pipeline은 후속이다.
- PR #4의 기존 Mac/Ubuntu fast CI 통과와 이번 로컬 검증은 구분한다. 이번 변경의 원격 CI 결과는 해당 PR의 revision별 검사 기록을 따른다.
