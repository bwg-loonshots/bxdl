# 구현 상태 — 2026-09-18

Rust·macOS Apple Silicon 우선 방향을 유지한다. R1 설정 초안 이후 **R2의 Mac 새 폴더 패키지 설치**와 **R3의 NIGO 개발 후보 정보·cold 사전검사**를 추가했다. clean NIGO 후보를 별도로 수신하고 제품 설정과 명시 native QBFT 설정을 연결하는 preflight를 추가했다. 이후 설치본·설정을 등록해 `--instance`로 preflight와 명시 init/resume-init을 호출하고, Mac 사용자 LaunchAgent의 start/status/stop을 연결했다. 실제 새 package의 단일 validator 서비스 수명주기를 확인했으며 setup에서 서비스 운용까지 이어지는 전체 UX·G1-M은 아직 완료하지 않았다.

## 실제 제공 기능

- `package build/verify`: 기대 hash·결정적 tar/gzip·외부 Ed25519 key·전체 파일 inventory·경로/형식/한도 검사.
- `install`: Mac arm64 개발 package를 새 명시 경로에 설치. 기존 대상 거부, 검증 중 파일 비실행, 최종 gzip 검사 뒤 receipt 기록. 중단 폴더를 자동 덮어쓰기·삭제·resume하지 않음.
- `engine inspect`: 신뢰 lock의 정확한 JAR/Java hash 및 engine-info build/source/console/contract identity 대조.
- `engine preflight`: 명시 NIGO native config/chain을 입력받는 cold JVM 검사. timeout/출력 한도·환경 분리, engine exit 3을 BXDL INCOMPLETE/exit 5로 보존.
- `config validate`/기본 `preflight`: BXDL 제품 설정·로컬 metadata 검사. 엔진 옵션이 없는 preflight는 엔진을 실행하지 않는다.
- 결합 `preflight`: 제품/local 검사, 명시 QBFT VALIDATOR/MTLS 설정과 ID·경로·endpoint·키 참조 일치, 같은 native snapshot의 pinned engine cold 검사. 제품/설정/chain hash와 결과를 연결한다. 로컬 FAIL·불일치는 JVM 실행 전에 거부한다.
- `setup`: 14개 제품 설정 입력·수정·checkpoint·재개·기존 JSON 가져오기·새 JSON 출력. 파일 저장 성공은 engine/service 준비 완료가 아니다.
- `instance register/show`: archive·외부 key·engine lock·전체 설치 manifest·제품/native/cold 결과를 고정해 private 등록 journal 생성, 저장 상태와 advisory busy 조회. 등록 후에도 원본 archive/key와 참조 자료가 필요하다.
- `preflight --instance`: 등록 입력 hash·전체 설치본을 재검증하고 기존 결합 cold 실행. 정상도 INCOMPLETE다.
- `init/resume-init`: 작업별 확인 flag, durable intent, inherited advisory lock, 단발 NIGO 호출, process exit/stdout/동일 attempt report/engine journal 대조. 불명 결과와 시도 자료를 보존한다.
- `start/status/stop`: Mac 사용자 GUI 세션의 시도별 LaunchAgent, 일회 gate와 같은 PID의 Java exec, report·launchd·로컬 HTTP 관측, 정상 종료 증명 후 등록 정리. 로그인 자동 시작·자동 재시작은 제공하지 않는다.

`logs/diagnose/upgrade/uninstall`은 미구현이다. 제품 config에서 NIGO QBFT peer/pin/validator 상세 설정을 추측 렌더링하지 않는다. `setup`의 instance.json과 `engine preflight`의 node.json을 구별한다.

## 새로 수신한 NIGO 결과

문서 HEAD `49d1cefc`(#119)에서 clean 인계 자료를 읽었다. source `303e163a9b3f293fa39e42d02b8daa1843973c14`, dirty=false인 실제 JAR `dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455`를 별도로 검증했다. [새 원문·공급자 증거 snapshot](../contracts/nigo/development-clean-2026-09-18/README.md)과 [소비자 실행 결과](../results/2026-09-18-clean-engine-preflight.md)를 따른다. 기존 dirty 후보 snapshot·결과는 당시 이력으로 보존했다.

#118의 QBFT drain·fresh sync·genesis-only observer·실제 source 종료 후 재선택 보완과 clean JAR 공급은 제공자 근거에서 확인됐다. BXDL에서는 같은 clean JAR를 담은 시험 Mac package를 새로 설치하고 **한 validator의 실제 QBFT cold 설정·NGVK/TLS 자료 검사**까지 확인했다. 이전 cold 검증은 peer 없는 정적 fixture였으며 당시 실제 4-node 운영·DB/native·launchd·G1-M은 수행하지 않았다. NIGO 저장소·요구 원장은 변경하지 않았고 REQ-0002 OPEN/계약 PROPOSED를 유지한다.

## 이전 등록·초기화 구현의 근거와 한계

새 경로의 실제 테스트·실행·중단 주입 결과는 [인스턴스 초기화 검증 기록](../results/2026-09-18-instance-initialization.md)에 별도로 기록한다. 위 이전 cold 결과를 DB 초기화·runtime·전체 G1-M 증거로 소급하지 않는다. 공급 source는 같은 clean `303e163a`이며 새로운 NIGO 계약/원장 변경은 없다.

초기화의 exit 0은 저장소 초기화와 종료 확인이다. launchd 등록·노드 시작·네트워크 정상·4-validator 인수가 아니다. timeout은 TERM 후 최대 5초 대기하며 SIGKILL하지 않아 child와 잠금이 남을 수 있다. UNKNOWN 뒤 자동 재시도·INITIALIZED 자동 채택은 없다. 원본 archive/key와 credential hash 고정, 매 작업의 전체 설치본 재검증을 유지한다.

제어 기록은 같은 UID의 로컬 파일이며 암호학적 상태 로그가 아니다. inode에 고정한 control 복사/이동·복원 절차와 수동 결과 채택은 미구현이다. 각 시도의 약 160 MB JAR snapshot·report는 남고 자동 GC는 없다. 세부 사용법은 [인스턴스 가이드](./instance.md), 상태 전이와 실패 정책은 [설계](../design/2026-09-18-instance-initialization.md)를 따른다.

등록·초기화 구현은 `make check` 162개 테스트, Mac release 빌드, Linux 타입 검사를 통과했다. 설치된 당시 Mac package의 실제 NIGO JAR로 초기화, 실제 controller crash에서의 잠금 유지·UNKNOWN 보존, 합성 중단 checkpoint에서의 실제 resume을 확인했다. [검증 기록](../results/2026-09-18-instance-initialization.md)에 시험 범위와 JRE 모듈 보완을 구분했다. PR #4의 Mac·Ubuntu fast CI 통과도 그 revision의 이력이며 이번 LaunchAgent 변경의 근거로 소급하지 않는다.

## 이번 LaunchAgent 구현의 근거와 한계

서비스 명령과 내부 worker gate를 구현했다. macOS 26.6.2 arm64의 실제 새 package에서 새 install/register/init 후 start/status·중복 start 거부·정상 stop을 확인했다. 같은 DB 재시작에서는 nodeInstanceId가 바뀌고 genesis가 유지됐다. start controller만 강제 종료한 시험에서도 LaunchAgent worker/Java가 계속 진행해 로컬 READY에 도달했고, 중복 start는 거부한 뒤 정상 stop했다. 소비된 attempt의 직접 kickstart 재실행 거부·데이터 hash 불변과 정상 종료 뒤 job 등록 해제도 확인했다. 이는 peer 없는 단일 validator의 서비스 시험이며 거래 확정·4-validator 인수가 아니다.

이번 `make check`의 fmt·Clippy·200개 테스트와 `make check-linux` 타입 검사는 통과했다. 원격 CI 결과는 해당 PR의 revision별 검사 기록을 따른다. 시험 경로·제약·실제 결과는 [LaunchAgent 검증 기록](../results/2026-09-18-macos-launchagent.md)을 따른다. Documents 아래 첫 시험은 worker 진입 전 exit 78로 종료했고 임시 경로로 입력을 분리한 시험은 동작했으나, OS 접근 제어/TCC를 원인으로 확정하지 않는다. 설계와 실패 정책은 [LaunchAgent 문서](../design/2026-09-18-macos-launchagent.md), 실제 사용법은 [인스턴스 가이드](./instance.md)를 따른다.

start에는 설치 package의 `bin/bxdl`과 동일한 바이트의 CLI가 필요하다. 기존 초기화용 package에 서비스 CLI만 덮어쓰는 방법은 사용할 수 없다. 새 archive·설치본의 전체 inventory를 검증하며 기존 등록/data의 migration은 후속이다. 시도별 worker/JAR snapshot과 private 로그는 보존하고 자동 용량 정리는 제공하지 않는다.

정상 STOPPED report·launchd의 프로세스 부재·control 잠금 획득을 함께 확인한 뒤에만 STOPPED_VERIFIED를 게시한다. status는 읽기 전용이며 복구 기록을 쓰지 않는다. gate 실패·UNKNOWN은 자동 init/repair/restart로 해결하지 않는다. stop timeout에 SIGKILL·live job bootout을 하지 않아 엔진과 잠금이 남을 수 있다. start timeout은 사전 검증 뒤의 시작 대기이며 실행 취소나 전체 명령 총 시간 제한이 아니다.

현재 profile은 수동 bootstrap한 사용자 LaunchAgent다. 자동 로그인 시작, 장기 운영 LaunchDaemon, 로그아웃/OS 종료의 escalation·sleep/wake 인수, 같은 package의 4-validator 거래/재시작과 전체 G1-M, 정식 JRE·최소 macOS 선정은 남아 있다. 같은 최종 package로 `Library/Application Support/BXDL` 아래 고유 시험 control/data/config에서도 register/init/start/status/stop을 확인했다. 공백 경로를 포함한 이 호스트의 결과이며 추가 OS·접근 정책 조합을 보장하지 않는다.

## 계획 대비 상태

| 작업 | 상태 | 남은 조건 |
| --- | --- | --- |
| BX-001/005 | IN_PROGRESS | 첫 Mac profile, 최소 OS/JRE/native·launchd 인수 |
| BX-002/010 | DONE | Rust 기반·CLI envelope·고정 toolchain/lock |
| BX-003 | PLAN | 정식 Java21 공급자·patch/hash·NOTICE/SBOM 선정 |
| BX-004 | IN_PROGRESS | clean 후보 수신·검증 완료, 전체 계약 합의/제품 인수 후속 |
| BX-011~014 | IN_PROGRESS | build/verify/새 폴더 설치 구현, 정식 공급·릴리스/업데이트 후속 |
| BX-020 | IN_PROGRESS | v1 제품/native 명시 설정 대조 구현, 자동 렌더·network/peer/pin 입력 확장 후속 |
| BX-021/022/024 | IN_PROGRESS | instance journal·명시 init/resume·결과 대조, gated run·정상 종료/같은 DB 재시작 확인. 전체 실패·복구/운영 인수 후속 |
| BX-023 | IN_PROGRESS | 제품/native cold·실제 RocksDB 단발 초기화 확인. 지속 runtime·DB/WAL 복구·service 인수 후속 |
| BX-030 | IN_PROGRESS | R1 초안·R2 파일 installer·독립 instance 등록/명시 초기화 연결. setup package 선택·자동 연결 후속 |
| BX-031/032 | IN_PROGRESS | 수동 LaunchAgent·status·정상 stop·restart 확인. 로그인/로그아웃·sleep/wake와 전체 사용자 lifecycle 인수 후속 |
| BX-033/034 | PLAN | logs/diagnose·안전한 제거, 지원 자료 정제·보존/용량 정책 |
| BX-040~043 | IN_PROGRESS | 단일 노드 서비스·같은 DB 재시작 확인. G1-M 전체 사용자 흐름·4-validator, Linux/Docker 별도 후속 |
| BX-044 | IN_PROGRESS | 이전 PR #4 fast CI 통과. 이번 fmt/clippy·200개 테스트·Linux 타입 검사 통과, 원격 CI는 해당 PR 기록 참조. 전체 package 인수·릴리스 pipeline 후속 |
| BX-050~053/M6 | PLAN | 버전 쌍 업데이트·offline 유지보수·고객 운영 확대 |

## 다음 순서

1. 새 Mac package에서 확인한 단일 validator lifecycle을 바탕으로 남은 중단·실패 경계와 사용자 세션 인수를 완료한다. 저장된 초기화 상태와 실제 노드 health를 분리하고 UNKNOWN 뒤 자동 재시작하지 않는다.
2. setup에 검증한 package 선택·명시 instance 등록과 NIGO native 설정 준비를 연결한다. QBFT membership·peer/pin·PKI/거래 fixture를 준비하며 자동 렌더 도입 시에만 새 schema/초안 migration을 설계한다.
3. 정식 Java 21 공급자·patch/hash·필요 runtime modules·NOTICE/SBOM과 최소 macOS를 선정한다. cold 실행 가능성이 init/runtime 가능성을 보증하지 않는다.
4. 선정 JRE와 같은 Mac package에서 동일 데이터 재시작·G1-M 전체 운영·4-validator 회귀를 인수한다. Linux/systemd G1-L, Docker G1-D는 뒤에서 별도 검증한다.

R2 파일 receipt는 instance 등록·NIGO journal이 아니며 전체 R2/R3 완료라고 표시하지 않는다. 현재 Mac 사용자 profile의 CLI·엔진은 같은 UID다. 파일 권한만으로 강한 격리를 보장하지 않는다. 기존 사용자 node/data/key는 시험하지 않는다.
