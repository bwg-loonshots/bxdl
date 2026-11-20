# 실제 Linux 우선 구현·인수 계획

## 공개 다운로드와 설치

[Linux 공개 시험판 구현 계획](./2026-11-18-linux-release-channel.md)에 서명된 사전 빌드 패키지 제작, GitHub Releases와 curl 설치 흐름을 추가했다. LNX-04/05는 전달 당시의 비공개 키트로 진행한다. 공개 태그의 실제 파일로 설치·재실행·노드 운용을 확인한 후 해당 draft를 공개한다.

## 현재 기준

Linux 기반·고정 공급물·개발용 systemd backend·이름 기반 설치/운용 UX와 비공개 시험 준비 도구를 구현했다. [PR #24](https://github.com/bwg-loonshots/bxdl/pull/24)의 main `00451dee657013b3ba3a793ddbf864ec66800f58` 기준 Ubuntu 24.04 CI에서 **native CLI 빌드·version 실행, Rust605 PASS, Python287개 중275 PASS·12 SKIP**를 확인했다. 11월 19일에는 Ubuntu 24.04.4 실기에서 RC.4 개발 후보의 빌드, 설치·init, 단일 노드 READY, SSH 단절 시 실행 유지, 재초기화 없는 재기동과 정상 종료를 확인했다. 권한 오류에서 도움을 받았고 설치 문답의 기본값은 미경험이다. 다음 키트에는 권한 복구, 상황별 다음 명령, 사건 목록과 진단 요약, helper 역할과 정리 안내를 함께 넣어 사용성을 한 번에 재검증한다. 정식 실행기 profile은 **NOT_ADMITTED**다.

실제 전달본은 해당 `HANDOFF.json`의 제품 source commit·지원 도구·공급물·plan/archive 해시로 식별한다. 과거 `ba4d5af` kit와 날짜별 검증 결과는 당시 이력으로 보존하고 최신 전달본의 source로 자동 사용하지 않는다. 현재 사용법과 지원 범위는 [Linux 설치·운용](../docs/linux-operations.md), [네이티브 빌드·후보 준비](../docs/linux-native-build.md), [지원 범위](../docs/support-matrix.md)를 따른다.

아래 단계별 상태는 각 단계 작성 시점의 기록이다. 최신 인수 범위는 위 기준과 [구현 상태](../docs/implementation-status.md)를 따른다.

## 2026-11-17 비공개 시험 범위 조정

- 빌드·후보 archive 제작은 사전 준비로 둔다. 실제 Ubuntu 24.04 x86_64/systemd 장비에서 같은 전달본으로 노드 설치·초기화·서비스 운용까지 시험한다. 기술 검증과 사용성 평가는 결과를 분리하며 둘 중 하나의 성공으로 나머지를 대신하지 않는다.
- `prepare-linux-operations.py`가 고정 build-plan/receipt와 로컬에서 직접 빌드한 verifier를 사용해 archive·전체 inventory를 확인하고 새 일반 사용자 폴더에 공급물을 추출한다. helper의 고정 root 경로 배치는 관리자 명령으로 명시하며 기존 파일은 덮지 않는다. generic `bxdl install`의 Linux signed profile 지원을 추가한 것으로 표현하지 않는다.
- `prepare-linux-test-inputs.py`는 선정 JRE/RC.4와 helper resource를 대조하고 시험 장비에서 fresh validator/TLS 자료를 생성한다. loopback의 manual peerless 단일 노드와 `/var/lib/bxdl/instances/<이름>/data`를 사용한다. 노드/DB/init/systemd는 실행하지 않는다.
- 본 과업은 setup 문답·중단/재개·조회 → apply → 명시 init → start/READY → logs/diagnose → SSH 연결 종료 후 같은 시도 유지 → stop → 같은 DB 재시작/정상 종료다. helper/설치 경로·계정은 기존 것과 충돌하면 보존하고 중단한다. 실패한 init을 지우거나 DB 초기화를 반복하지 않는다.
- `UAT` 폴더는 삭제한다. 안내서·빈 양식·개별 시험 결과·전달 archive는 Git 밖의 비공개 패키지로만 관리한다. 제품 source commit·지원 도구 hash·JRE/JAR pin과 실행 미완료 범위를 `HANDOFF.json`에 고정한다. 테스트 결과의 공개나 NIGO 전달은 별도 요청 때 정제해 다룬다.
- 네트워크 정족수·거래·재부팅/자동재시작·별도 버전 관리 CLI 전환·WSL2는 후속이다. 최초 시험의 A/C bytes가 같다는 사실을 CLI 버전 전환 인수로 계산하지 않는다. 실제 Linux 관측 전에는 서비스 인수 PASS/exact profile 승인을 기록하지 않는다.

- 결정일: 2026-11-16. 반복 검토자가 실제 Linux 머신을 사용할 수 있으므로 **실제 Linux → Linux 사용자 과업·서버 인수 → WSL2 추가 검증** 순서로 변경한다.
- 갱신: 2026-11-17. 현재 상태와 CI 근거는 위 “현재 기준”을 따른다. [LNX-01 결과](../results/2026-11-16-linux-foundation.md)는 당시의 Mac 회귀·Linux target 타입 검사 기록이며, 이후 Ubuntu CI의 native CLI 빌드 성공이나 아직 미실행인 노드·서비스 인수를 소급 기록하지 않는다.
- 연결: [전체 구현 계획](./2026-09-16-implementation-plan.md), [인수·릴리스 기준](./2026-09-16-acceptance-and-release.md), [CLI 과업](./2026-10-22-integrated-operations-ux-implementation-plan.md), [지원 범위](../docs/support-matrix.md).

## 1. 방향 변경과 보존할 결과

Mac 우선 결정은 운영자가 실제로 사용할 수 있는 개발 환경에서 UX를 검증하기 위한 선택이었다. 이후 Mac RC4-01~06의 지정 조합 기술 인수와 RC4-07 정식 회신 게시를 완료했다. 새 반복 테스터가 실제 Linux를 사용할 수 있으므로 다음 개발과 사용자 피드백의 기준을 Linux로 바꾼다.

Rust, 직접 CLI, 순차 설치 문답, 노드 이름 기반 운용, 관리 CLI와 노드 runtime의 독립 버전 관리는 유지한다. 공통 CLI 개선은 Mac에서도 회귀 검사한다. `bxdl console`의 개발 중단 결정도 유지한다. 자동완성은 현재 구현한 zsh만 유지하며 Linux 지원 추가가 bash/fish 자동완성 지원을 뜻하지 않는다. 실제 테스터의 shell과 안내서 적용 결과를 기록한다.

[RC4-06 결과](../results/2026-11-16-rc4-macos-network.md)·[정제 JSON](../results/2026-11-16-rc4-macos-network.json), Mac runtime R·내부 실행기 A·관리 CLI B/C의 고정 bytes와 [NIGO PR #257](https://github.com/bwg-loonshots/nigo-protocol/pull/257)은 이전 범위의 근거로 보존한다. 새 Linux 구현을 그 당시 결과에 소급하지 않는다. Mac RC4-08/CLI05-04 사람 과업은 **NOT_RUN·우선순위 이연**이다. Linux 사람이 수행한 결과는 Linux profile에 기록하고 Mac 과업을 대신 완료하지 않는다.

## 2. 첫 대상과 환경 확인

| 항목 | 초기 기준·확정 조건 |
| --- | --- |
| 주 검증 환경 | 실제 Linux 머신. 호스트/CPU·배포판·kernel·glibc·systemd를 기록하고 실제 실행 근거를 남김 |
| 사용자 허용 첫 기준 | Ubuntu 24.04 LTS·x86_64/amd64·glibc·systemd·sudo/SSH 가능 조건. Ubuntu CI의 CLI 검사는 완료했지만 테스터 장비의 배포판/권한·systemd 실행 관측은 아직 없음. 다른 배포판·ARM64를 자동 포함하지 않음 |
| Java·저장소 | Linux Temurin21.0.13+9 full JRE·고정 RC.4 JAR·RocksDB. 공급물 lock·inventory는 선정 완료, 해당 조합의 실제 Java/native/RocksDB 실행 인수는 대기 |
| 접근·권한 | sudo/SSH 가능은 사용자 답변에 따른 전제다. 실제 접속 대상·권한·OS 정보는 접속 뒤 관측하며 비밀번호·개인키는 문서나 Git에 수집하지 않음 |
| 시험 입력 | 새 test-owned control/data·시험용 signer/TLS·명시 포트. 기존 운영 DB·노드·계정으로 테스트하지 않음 |
| Mac | 기존 지원 조합과 실제 결과 유지, 공통 Rust/CLI 회귀 환경 |
| WSL2 | LNX-07 후속. Linux 실제 머신의 systemd·서버 인수를 대체하지 않음 |
| 이번 범위 밖 | Windows native, Linux ARM64, Alpine/musl, Docker, 기존 DB migration·혼합 버전 rolling upgrade |

Ubuntu CI에서는 실제 native CLI compile/link와 `version` 실행을 확인했다. 테스터 장비에서의 JRE/엔진·설치·서비스 실행은 아직 관측하지 않았다. SSH 정보 공유는 필수가 아니며 테스터가 비공개 전달본의 절차로 직접 실행하고 선택한 결과를 반환할 수 있다. cold identity만 별도 확인할 때는 [Linux 실행 검증 안내](../docs/linux-validation.md)를 따른다. 구현·합성 회귀·target 타입 검사·CI의 native CLI·실제 노드/서비스를 구분한다. Mac cross-check나 합성 fixture PASS를 Linux linking·systemd PASS로 확대하지 않으며, Ubuntu CI의 linking 성공도 엔진·서비스 실행의 근거로 사용하지 않는다. 대상 Linux 정보가 달라지면 코드의 임의 허용 범위를 넓히기 전에 지원 후보와 인수 항목을 갱신한다.

## 3. 단계와 완료 기준

| 단계 | 구현·산출물 | 완료 기준 | 현재 상태 |
| --- | --- | --- | --- |
| LNX-01 환경·플랫폼 기반 | Linux runtime inventory staging, Linux 관리 CLI bundle 제작/검증/설치·업데이트 경계, 읽기 전용 host/systemd 준비 상태 검사, 플랫폼별 실패 안내 | Linux와 Mac의 OS/CPU·신뢰·경로 계약 회귀, Linux target 타입 검사, host 검사 무변경·부분 결과 검증. 실제 Linux 실행은 별도 표기 | IMPLEMENTED / CI_NATIVE_CLI_CHECKED — Ubuntu CI의 native CLI 빌드·version/자동 검사 PASS. 사용자 배포·설치와 실제 노드/서비스 인수는 별도 대기 |
| LNX-02 Linux 고정 공급물 | 선정 Linux JRE 전체 inventory·provenance, 고정 RC.4 JAR의 engine-info/native/RocksDB 사전 실행, Linux runtime R-L·내부 실행기 A-L·관리 CLI C-L의 분리 identity | 실제 Linux에서 JRE/JAR·전체 engine identity·inventory 대조, 고정 package 검증. clean source·archive/manifest/JAR/Java/CLI hash를 기록 | SUPPLY_PINNED / OFFLINE_VERIFIED — 실제 선정 Java/native/RocksDB와 고정 runtime package 제작·수신 NOT_RUN. 일반 native CLI 빌드는 Ubuntu CI PASS |
| LNX-03 systemd 서비스·권한 | system service adapter, instance별 전용 UID, root 제어 기록, 서비스 시작 gate·시도별 identity, 정상 종료 판정과 정제 로그 | OS 증거와 engine 보고서 결속, 서비스 직접 실행도 gate 적용, 미완료/불명 init 거부, 다른 instance/UID 무영향 및 엔진의 제어 기록 변조 거부 | DEVELOPMENT_BACKEND_AUTO_CHECKED — 당시 [검사 결과](../results/2026-11-16-linux-systemd-development.md)와 이후 Ubuntu CI 회귀. 실제 계정·init·systemd 실행 NOT_RUN |
| LNX-04 설치·운용 UX와 실제 단일 노드 인수 | Linux package 조립·고정, 고정 실행기 위임, 전용 계정 생성/설치 승인·이름 연결 UX와 전체 bounded 진단 수집 통합, 설치→명시 init→start/status/logs/diagnose/stop, 같은 DB 재시작, SSH 연결 종료 뒤 관측, 실행 중 관리 CLI 전환 | 같은 고정 Linux 조합의 원자료·정제 결과, 정상 정리·payload/worker 불변·DB identity 유지. 단일 노드 로컬 readiness와 전역 합의 구분 | DEVELOPMENT_UX_IMPLEMENTED / HOST_ACCEPTANCE_NOT_RUN — 사용자 준비·root 설치 승인·이름/최초 worker 위임·진단 연결. 고정 source kit·제작/수신·설치 입력/시험 키 준비 도구 PREPARED. CI native CLI 빌드는 PASS, 고정 runtime 후보·노드/서비스 인수는 NOT_RUN, profile NOT_ADMITTED |
| LNX-05 Linux 사용자 과업 | 비공개 package·외부 checksum/lock·장비에서 생성한 시험 입력·안내서·결과/개선 양식. 작성자가 아닌 테스터의 직접 실행 | 빌드를 사전 준비로 두고 설치·중단/재개·init·서비스·진단·SSH 재접속·정상 종료·같은 DB 재시작. 기술 결과와 편의성 별도 기록 | PRIVATE_HANDOFF_PREPARED / NOT_RUN |
| LNX-06 Linux 서버·네트워크 인수 | 동종 4-validator 거래·두 정족수 복구·wrong-pin, systemd lifecycle·재부팅 정책·전용 UID 격리·다중 host | G1-L의 해당 gate별 실제 근거. 로컬4노드와4-host를 구분하고 로그아웃/SSH 단절·재부팅·장애 범위 명시 | PLAN / NOT_RUN |
| LNX-07 WSL2 추가 검증 | 지원 Linux 배포본을 WSL2에서 설치·운용, shell/PATH·filesystem·network·WSL 종료 조건 확인 | Windows/WSL/kernel/배포판/CPU/systemd 조합을 고정한 별도 결과. Linux 서버 PASS를 자동 승계하지 않음 | DEFERRED / NOT_RUN |

LNX-04 실물 기술 검증과 LNX-05 사용성 관측은 같은 비공개 전달본에서 단계별로 수행한다. 선행 기술 단계가 실패하면 의존 단계는 중단하고, 이미 경험한 준비·설치 UX의 막힘과 의견은 그대로 회수한다. UX 코드·합성 테스트만으로 실제 운영 과업 PASS를 기록하지 않는다. 모든 G1-L 장기·다중 host 검증을 기다린 뒤에만 UX 피드백을 받도록 묶지 않는다. 반대로 초기 사용자 과업 PASS를 LNX-06 완료나 정식 고객 배포로 해석하지 않는다. 각 단계는 실제 구현·자동 검사·host 실행·공급 확인을 별도 상태로 남긴다.

## 4. 재사용할 CLI와 새 플랫폼 경계

사용자 명령은 `setup new/resume/list/show/edit/apply`, `nodes`, `init`, `start/status/stop`, `logs/diagnose`, `cli install/update/info/rollback`을 공통으로 유지한다. Linux에서도 준비 저장·파일 설치·DB 초기화·서비스 시작의 승인 범위를 나눈다. 필요한 권한과 대상 경로를 실행 전에 설명한다.

| 공통으로 유지 | Linux에서 구현·확인 |
| --- | --- |
| 서명·inventory·archive 검증, 명시 engine/JRE pin | Linux executable/JRE·glibc·native와 실제 실행 target |
| 순차 문답·취소·재개·한 항목 수정, 노드 이름 목록 | Linux 기본 관리 경로, 설치/운용 권한과 서비스 계정별 경로 |
| 고정 runtime 내부 실행기에 위임하는 관리 CLI | Linux exact profile은 실제 고정 공급물 인수 후 추가. Mac profile을 OS만 바꿔 재사용하지 않음 |
| init/resume-init의 명시 승인·중단 기록 | 비root engine 초기화와 root 제어 기록 사이의 권한 경계 |
| readiness·보고서·종료 결과와 UNKNOWN 의미 | systemd unit/InvocationID/cgroup/PID와 실제 엔진 보고서의 동일 시도 결속 |
| 제한된 logs/diagnose와 정제 정책 | Linux log 접근·소유권·저장 위치, 서비스 중단 뒤 오프라인 진단 |

대규모 plugin framework를 먼저 만들지 않는다. 기존 launchd의 프로세스 관측·서비스 동작을 필요한 최소 adapter 경계로 분리한다. 공통 상태에 GUI domain이나 systemd unit을 강제 공통 필드로 넣지 않으며, 플랫폼별 관측을 해당 adapter에서 검증한 뒤 공통 결과로 표현한다.

`host check`는 **읽기 전용 준비 상태 검사**이며 unit 생성·등록·daemon-reload·start/stop·사용자 생성·권한 변경·sudo 설정을 수행하지 않는다. host 성공은 노드 READY가 아니다. LNX-01 당시 서비스 경로는 미지원이었으나 현재 LNX-03의 system service backend와 LNX-04의 명시 설치·운용을 구현했다. 이를 user service로 우회하지 않으며 실제 시스템 권한·서비스 인수는 별도로 수행한다.

### LNX-01 읽기 전용 host 검사와 runtime staging

`bxdl host check [--timeout-seconds <1..30>] [--json]`은 host 기준만 읽고 systemd의 system manager를 조회한다. 기본 제한은 5초다. 선택 기준은 Linux GNU/x86_64·Ubuntu 24.04·PID 1 systemd·systemd 255 이상·manager의 `running` 상태다. OS/CPU·배포판·kernel과 WSL 여부, PID 1·manager version/state를 관측한다. 실제 glibc 버전·sudo 권한·JRE/RocksDB·서비스 계정 경계는 이 명령의 검사에 포함하지 않는다.

JSON `data`의 `outcome`/`reasonCode`와 상위 결과의 이유를 구분해 읽는다. 기준을 관측하면 `SUCCEEDED / HOST_BASELINE_OBSERVED`(exit 0), 다른 플랫폼·선정 밖 배포판/버전·PID 1/systemd 상태·WSL은 해당 이유를 가진 `INCOMPLETE`(exit 3)다. 조회 자체의 실패는 오류로 반환한다. WSL은 `HOST_WSL_DEFERRED`로 별도 표기한다. LNX-01 당시 `linuxServiceControl=NOT_IMPLEMENTED`였으며 LNX-03에서는 `DEVELOPMENT_BACKEND_IMPLEMENTED`로 표시한다. `engineAcceptance=NOT_RUN`, `serviceMutations=0`은 유지하므로 host 성공을 서비스/엔진 인수 성공으로 해석하지 않는다. node HTTP 호출·서비스 변경·sudo 요청·파일 생성은 수행하지 않는다.

`runtime stage`는 기존 `darwin/arm64`와 새 `linux/amd64`·`libc=glibc`·Java21 lock을 허용한다. Linux에는 명시 `libc`가 필요하고 Mac lock에는 이 필드를 넣지 않는다. 기존 Mac lock의 형식·bytes를 자동 수정하지 않는다. 이 기능은 offline inventory·안전한 symlink materialization·hash/mode·provenance 검증이다. Linux JRE/lock 선정은 LNX-02에서 완료했으며, runtime staging 성공이 archive의 원격 출처 인증·Java 실행·최종 runtime package 인수를 대신하지 않는다.

### LNX-01 관리 CLI 배포 구현 경계

Linux 관리 CLI의 초기 target은 `linux-amd64`(Rust `x86_64-unknown-linux-gnu`)다. Linux GNU/x86_64 host에서 실행한 `cli install/update/rollback` 경로를 구현하며, 다른 OS/CPU 배포본의 선택·복귀는 거절한다. Linux ELF64 little-endian·x86_64와 glibc interpreter를 검사하고 Windows/ARM64/musl 지원을 의미하는 capability를 선언하지 않는다. Ubuntu CI의 native CLI 빌드·version 실행은 PASS다. 실제 사용자 CLI 배포·설치/업데이트·운영 노드와의 조합 인수는 아직 NOT_RUN이다.

기본 관리 CLI root는 `$HOME/.local/share/BXDL-CLI`, 실행 진입점은 `$HOME/.local/bin/bxdl`이다. `--root`/`--bin-dir`로 명시할 수 있으며 이번에는 XDG 환경변수에 따른 자동 경로 변경을 도입하지 않는다. 이 사용자별 CLI 저장소는 LNX-03의 시스템 노드 설치·root 제어 기록·서비스 UID 저장소와 별개다. 기존 Mac 기본 위치는 유지한다.

공통 노드 목록·준비/실행 기록의 Linux 기본 관리 home은 `$HOME/.local/share/BXDL`이며 `--home`으로 지정할 수 있다. `nodes`·이름 기반 명령·순차 준비가 같은 사용자 catalog를 사용하도록 한다. CLI 배포 저장소인 `BXDL-CLI`와 분리하고 XDG 환경변수를 자동 채택하지 않는다. 이 catalog 기본 경로의 추가는 시스템 노드의 privileged control/data를 사용자 home에 배치한다는 뜻이 아니며, LNX-03에서 설치·운용 권한과 경로를 별도로 인수한다.

LNX-04의 Linux `setup new/resume/list/show/edit/apply`는 사용자 home의 `sessions/<이름>`을 사용하고 시스템 control/data는 `/var/lib/bxdl/instances/<이름>` 아래에 둔다. 아래의 기존 `setup --workspace`는 이 새 Linux 설치 문답과 별도인 설정 초안 기능이다. Linux 기본 home 적용은 이름 기반 `setup`·`nodes` 등의 catalog 경로다. 기존 `bxdl setup`의 생략 `--workspace` 기본값은 아직 Mac 전용이므로 Linux에서는 `--workspace`를 명시한다. Linux 초안의 `dataDirectory` 제안은 입력 기준 디렉터리의 `bxdl-instances/<id>/data`이며 system service의 실제 설치·권한 layout을 확정한 것이 아니다. Linux 서버 control/data 경계는 LNX-03/04에서 구현했으며 실물 인수는 별도로 수행한다.

제작 명령은 `python3 tools/build-management-cli.py --target linux-amd64 --output <새-archive> --signing-key <private-key>`다. target을 생략하면 현재 native host의 기본 target을 선택하며 Mac 기본은 기존 `darwin-arm64`다. Linux target 제작·후보 probe는 native Linux GNU/x86_64 host에서 수행하고 Mac에서 cross-built ELF를 실행하지 않는다. 서명 없는 개발 제작은 기존 명시 개발 정책을 따른다.

Linux 관리 CLI 설치·업데이트 capability와 Linux 노드 엔진 exact profile·서비스 capability는 분리한다. 새 관리 CLI가 존재한다는 이유로 Linux RC.4 package의 내부 실행기를 임의 인증하거나 Linux start/stop를 허용하지 않는다.

## 5. systemd 시스템 서비스 설계

Linux 서버 profile은 **systemd system service + instance별 비root 실행 UID + root가 보호하는 제어 기록**을 기준으로 한다. Mac의 동일 UID LaunchAgent 모델과 Linux의 권한 보장을 구분한다.

1. 관리자는 검증한 immutable package와 unit·control 기록을 보호한다. 엔진 계정은 자신에게 필요한 data·지정 output만 쓸 수 있어야 한다. 엔진의 root journal·다른 instance 자료 수정 거부를 실제 시험한다.
2. 권한이 필요한 설치·service 등록과 일반적인 조회/운용의 허용 범위를 구분한다. 무제한 passwordless sudo나 범용 root 실행 daemon을 추가하지 않는다. 운영자가 자의적인 shell command를 privileged 경로에 전달하지 못하게 한다.
3. `bxdl start`와 `systemctl start`가 같은 artifact·설정·init 완료·미해결 작업 검사를 통과해야 한다. 서비스가 root JVM으로 실행되지 않으며 init 경로도 동일 실행 계정의 소유권과 데이터 조건을 검증한다.
4. startup 요청, single-use attempt gate, unit/InvocationID·cgroup·PID, engine nodeInstanceId와 보고서를 결속한다. 이름이 같은 다른 서비스나 재사용 PID를 종료 대상으로 삼지 않는다.
5. OS가 inactive/failed라고 보고한 사실만으로 정상 종료를 인정하지 않는다. 엔진의 STOPPED·storage/consensus 종료 근거, process/cgroup 부재, DB lock 해제 조건을 함께 확인한다. timeout·관측 불명은 UNKNOWN으로 보존한다.
6. 첫 서비스 단계의 자동 재시작은 기본 OFF다. 로그인/SSH 세션과 독립인 시스템 서비스 수명주기를 검증하고, 부팅 자동 시작은 명시 정책·startup gate·실제 재부팅 인수를 추가한 뒤 지원한다. 직접 kick/restart가 이전 승인을 재사용하지 않게 한다.
7. root-owned 기록과 runtime/engine-owned output을 연결하는 프로토콜은 LNX-03에서 별도 회귀·실물 인수한다. 기존 Mac control의 owner 검사를 제거하거나 모든 UID를 허용해 Linux 동작을 만드는 변경은 하지 않는다.

### LNX-03 개발용 경로와 설치 UX 연결 경계

기존 Mac control은 같은 UID를 전제로 하므로 owner 검사를 확장하지 않는다. 별도 Linux control은 새 root0755 디렉터리에 root0600 binding·private journal, root 소유 runtime/JAR/worker/config, 서비스 UID0700 data/output을 둔다. Linux 전용 UID/GID는 경로에서 계산한 `bxdl-<digest>` 계정으로 한정한다. 저수준 직접 `linux-service register`는 관리자가 미리 생성한 nologin 계정·전용 그룹을 검증한다. 현재 일반 사용자 `setup apply`는 명시 승인한 설치 단계에서 전용 계정을 준비한다. sudoers는 자동 수정하지 않는다.

개발 명령 `linux-service register --instance <새 경로> --spec <json> --allow-development`가 고정 Linux lock·공급 inventory·설정을 검증·보관하고 서비스 UID로 cold 검사를 한다. 이후 `init/resume-init/start/status/stop --instance`와 offline 기록 경로를 연결한다. LNX-03의 명시적인 관리자 경로이며 기존 `setup apply`/`nodes add`를 Linux 시스템 설치 UX로 확장한 것은 아니다. LNX-04에서 계정/경로 검토·설치 승인·이름 기반 위임을 연결했다. signed package 조립·exact profile과 실제 Linux 인수는 남아 있다.

새 root 소유 gate binary를 시도별 unit이 실행한다. `Type=exec`는 gate 실행 관측이며 Java READY가 아니다. gate는 root 기록을 소비하고 unit/InvocationID/PID/cgroup을 결속한 뒤 고정 `/usr/bin/setpriv`로 보조 그룹·capability를 제거하고 전용 UID/GID로 Java를 exec한다. cold/init도 같은 권한 하향 경계를 사용한다. root journal의 쓰기 descriptor를 엔진에 전달하지 않으며 읽기 전용 operation lock만 stdin으로 상속한다. 자동 restart/enable·범용 shell·임의 Java 옵션은 제공하지 않는다.

종료 직후 `ExecStopPost`가 root 소유 OS 종료 기록을 남겨 systemd GC 이후에도 InvocationID·PID·정상 exit를 검증한다. STOPPED 판정은 이 기록과 engine 보고서의 storage/consensus closure와 root가 결속한 시도의 process/cgroup 부재·DB lock 획득을 함께 요구한다. 종료 불명·gate 중단·권한/자료 불일치를 과거 정상 상태로 덮지 않는다. 실제 Linux에서 계정 격리·잠금·직접 systemctl 재실행·SSH 단절을 확인하기 전에는 systemd 인수 PASS를 기록하지 않는다. [개발 경로와 실제 시험 목록](../docs/linux-service-development.md)을 따른다.

### LNX-04 구현한 준비·권한·위임 계약 — 2026-11-17

- `setup new/resume/list/show/edit/apply`와 이름 기반 `init/start/status/stop/logs/diagnose`를 연결한다. 변경 문답·apply는 실제 Linux의 일반 사용자 TTY에서 실행하며 Mac의 `--linux`는 읽기 전용 list/show와 도움말에 한정한다. `--runtime`은 선정 JRE를 symlink 없이 준비한 flat 폴더, `--jar`는 고정 RC.4, `--native-config`는 기존 설정이다. signed runtime archive 설치로 표현하지 않는다.
- 사용자 `sessions/<이름>`에는 부분 문답·검토 revision·생성 제품 설정/설치 요청·apply 시도와 완료 요약을 보관한다. root는 사용자 catalog를 열거나 쓰지 않는다. 처음 이름/데이터 위치를 하나의 draft checkpoint에 함께 넣고 재개 시 대조한다. 선택 항목의 `None`은 생략해 strict JSON 재개가 가능하도록 했다. 같은 값·잘못된 입력은 검토를 유지하고 실제 수정은 이전 검토를 해제한다. apply 시도 뒤 입력은 동결한다.
- 관리자 사전 준비는 고정 `/usr/local/libexec/bxdl/bxdl` 실행기다. root 소유·보호된 조상 경로와 실행 파일을 검증하며 사용자 CLI를 임의 root 실행하지 않는다. apply 승인 이후 sudo 인증과 immutable 요청/hash 대조를 거쳐 전용 계정·root 공급물 사본·cold 검사·등록을 수행한다. 계정 생성/부분 실패 기록은 보존하고 자동 삭제·덮어쓰기하지 않는다.
- 등록의 `management.json`은 공개 root descriptor다. control의 device/inode, protocol과 최초 worker의 hash/size를 포함하며 사용자 catalog가 descriptor digest를 고정한다. 일반 사용자는 이 공개 자료만 읽고 private binding/journal은 읽지 않는다. 새 관리 CLI가 지원하는 고정 프로토콜로 최초 root worker에 위임하며 임의 shell/Java/다른 worker 경로를 전달하지 않는다. 이는 개발용 root 신뢰 경계이며 정식 signed runtime exact profile 승인이 아니다.
- init/start는 apply에 포함하지 않는다. 목록의 `SYSTEM_REGISTERED`는 저장 당시 등록 의미이며 초기화·READY를 주장하지 않는다. 제한된 정제 logs/diagnose는 초기화·실행 보고서와 root 기록을 수집하고 한도·누락은 partial로 보존한다. HTTP·systemctl·JVM을 새로 실행하는 health 검사로 표현하지 않는다. diagnose의 정제 파일은 호출 사용자 소유 새 파일로 생성한다.

[Linux 운용 안내](../docs/linux-operations.md)에 현재 명령·경로·bootstrap 전제를 기록한다. Mac의 합성 user-journal 9사례와 target/catalog 집중 회귀는 실제 Linux 인수가 아니다. 자동 검사·Linux 타입 검사·Ubuntu CI의 실제 native CLI 빌드를 구분해 기록하며, 어느 결과도 JRE/노드/systemd 실물 인수를 대신하지 않는다.

LNX-04의 남은 순서는 (1) 준비한 고정 kit를 실제 Linux에서 빌드해 unsigned 개발 CLI/runtime archive·receipt를 제작하고 독립 수신 검증, (2) 선정 JRE/RC.4 cold/native 조합 확인, (3) 정확한 조합에서 설치·계정 격리·명시 init·서비스·정상 종료·같은 DB 재시작·SSH 단절·관리 CLI 전환 인수와 Linux runtime R-L/A-L 실행기 profile 승인 근거 확정이다. 개발 archive 제작과 hash 고정만으로 설치·서비스 profile을 승인하지 않는다. LNX-05 전달본은 빌드를 사전 단계로 두고 설치·초기화·단일노드 운용까지 실행할 수 있게 준비하며, 실제 인수 결과는 테스터 관측 뒤에 확정한다. 현재 새 NIGO JAR나 API 구현을 선행 요청하지 않으며 실제 결함·계약 불일치가 관측될 때 별도 근거로 협의한다.

## 6. JRE·엔진 공급과 패키지 고정

NIGO RC.4 source `037c98c315d5322fa7da266ca0254d61f3863622`, JAR SHA-256 `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a`를 새 Linux 조합 검토의 출발점으로 삼는다. 같은 JAR가 Linux에서 적합한지는 Linux JRE·native 실행과 전체 engine-info로 확인한다. 새 JAR 제작을 기본 선행 조건으로 두지 않는다.

Mac용 JRE archive·Java launcher·runtime R은 Linux 공급물이 아니다. Linux JRE의 공급자·patch·CPU·archive/source hash·전체 inventory와 문서/NOTICE를 새 lock으로 확정하고 Linux package를 새로 조립한다. 현재 Mac Temurin 선정은 참고 이력이며 Linux에서의 자동 승인 근거가 아니다.

BXDL은 Linux JRE 선정·패키지 구성·권한·systemd·실제 조합 인수를 담당한다. NIGO에는 선택한 Linux/Java/native/RocksDB 실행 조건과 공급자의 지원·검증 범위를 확인한다. RC.4의 Mac 공급·인수 회신을 Linux 확인 회신으로 확대하지 않는다. 엔진 결함·missing native·계약 불일치가 확인되면 정확한 JAR/JRE/OS identity와 재현 근거로 요청하고 기존 후보 bytes는 보존한다.

Linux runtime R-L/내부 실행기 A-L과 외부 관리 CLI C-L을 별도 고정한다. C-L만 변경하면 같은 R-L을 재사용하되 지원 profile과 영향받는 실제 조합을 검증한다. 초기 Linux 패키지가 아직 고정되지 않은 동안 임의 Linux package의 내부 실행기 위임 capability를 선언하지 않는다. 배포본 검증·CLI 실행 성공과 엔진 지원은 별도다.

NIGO 문서를 수정할 필요가 생기면 전용 worktree와 브랜치를 사용하고 한국어 PR로 공유한다. 현재 Linux 계획 수립만으로 NIGO requirements 수신·동의, 새 engine 후보 또는 JRE 공급자 검증 완료를 기록하지 않는다.

### LNX-02 공급물 선정 결과와 남은 실행

2026-11-16에 Linux용 **Temurin21.0.13+9-LTS full JRE**를 별도로 선정했다. 공식 archive·source·build 자료 수신과 정규화257파일 inventory를 고정하고 Python 준비 도구와 LNX-01 clean CLI의 Rust staging으로 교차 검증했다. 고정 RC.4 JAR/expected identity는 유지하고 Linux Java hash를 새 engine lock에 결합했다. [후보 identity](../packaging/linux-rc4-candidate.json)와 [공급 결과](../results/2026-11-16-linux-rc4-supply.md)를 따른다.

LNX-02 전체 완료를 뜻하지 않는다. Ubuntu CI의 CLI 빌드 성공과 별도로, 선정 Linux JRE의 properties/modules·engine-info는 probe로 수행하고 native/RocksDB는 명시 exact-JAR/selected-JRE 시험 입력·결과로 확인해야 한다. cold probe PASS를 RocksDB 실행으로 계산하지 않는다. Linux R-L/A-L/C-L byte identity는 해당 고정 kit의 native 빌드·조립 receipt로 확정하고, 실행기 profile은 별도 서비스 인수 후 승인한다. Mac runtime의 pin이나 과거 LNX-01의 서비스 미구현 CLI를 운용 인수된 Linux 실행기로 사용하지 않는다.

### LNX-04 native build·후보 고정과 전달 기준

[네이티브 빌드 안내](../docs/linux-native-build.md)의 도구와 비공개 전달본을 사용한다. **CLI의 Ubuntu CI native 빌드는 PASS**, 고정 JRE/RC.4 runtime 후보 제작·수신과 해당 노드·systemd 인수는 **NOT_RUN**이다. CI용 CLI 실행 파일을 테스터의 runtime 후보로 자동 채택하지 않는다.

1. 전달본 `HANDOFF.json`이 지정한 committed source archive와 `Cargo.lock`·Rust 1.86.0·Linux JRE/RC.4 lock을 사용한다. PR #24 머지 후 전달본은 위 main 기준으로 갱신했으며 source 내용과 검사된 PR head의 동일성을 확인했다. 이후 전달본도 각자 기록한 commit을 기준으로 한다.
2. 지원 도구의 전체 파일 hash, 외부 전달 ZIP/kit/plan SHA-256을 대조한다. 문서의 과거 commit이나 현재 checkout의 미커밋 파일을 kit에 섞지 않는다. 제품 source 변경을 포함하려면 새 commit으로 전달본을 다시 만들어야 한다.
3. Ubuntu 24.04 GNU/x86_64 일반 사용자가 kit로 native CLI와 unsigned 개발 runtime/관리 CLI archive·receipt를 만든다. 빌드 단계에는 JVM·root 설치·systemd 실행이 없다. 이 고정 공급 조합의 제작 결과와 binary/archive hash는 아직 테스터 실행 대기다.
4. 후보는 독립적으로 신뢰한 로컬 verifier로 plan·receipt·archive/manifest/payload pin을 확인한다. 별도 장비에서 수신한 Linux 실행 파일을 그 자체의 검증기로 사용하지 않는다. 같은 장비에서 직접 빌드한 verifier를 명시 hash로 고정한 뒤 설치 입력 추출에 사용하는 경로는 별도로 제공한다.
5. `prepare-linux-operations.py`로 검증된 설치 입력을 추출하고, `prepare-linux-test-inputs.py`로 시험 장비 안에서 새 단일노드 자료를 만든다. 이후 명시 helper 배치·setup/apply·init·서비스 시험을 진행한다. 준비 결과가 맞아도 실제 실행 단계는 수행한 범위만 기록하며 exact executor profile은 별도 인수 전까지 **NOT_ADMITTED**다.

LNX-02 최초 전달본, `ba4d5af` 기준 초기 kit와 날짜별 공급/검증 결과는 당시 이력으로 보존한다. 새 kit를 과거 archive 경로에 덮거나 이전 기록을 새 source의 결과로 바꾸지 않는다. 기존 Mac 고정 runtime/profile도 변경하지 않는다.

## 7. 실제 Linux·사용자 검증

LNX-04 실물 검증은 새 설치로 시작한다. 첫 비공개 과업은 자료 선택·취소/재개·조회·apply 승인, init과 start의 별도 명령, 상태/로그/진단, 정상 stop·같은 DB 재시작을 확인한다. 계정·경로·port·unit 이름은 시험별로 고정하고 기존 서비스를 건드리지 않는다. 별도 버전의 CLI 전환과 그 전후 node/PID/DB/worker 보존은 후속 인수이며 첫 전달본의 동일 A/C bytes를 CLI 전환 증거로 계산하지 않는다.

SSH 연결 해제 뒤에도 같은 서비스/engine 시도가 유지되는지 확인한다. SSH client 연결 종료, 사용자 로그아웃, OS 재부팅, systemd stop, 강제 종료와 정전 복구는 서로 다른 사례다. 한 사례를 실행하고 다른 사례도 통과했다고 기록하지 않는다.

LNX-05는 terminal 경험은 있지만 BXDL 작성자가 아닌 사람이 안내서·배포물로 수행한다. 설치 전 항목 수정, 준비 중단 후 재개, 다음 날 새 터미널에서 같은 노드 조회·정상 중지/재시작, 준비된 오류의 해석·진단 생성을 평가한다. 명령을 외워서 수행하는지보다 필요한 행동과 상태를 스스로 찾는지 기록한다. 완료·도움 받아 완료·막힘, 소요 시간, 혼동 문구, 시도한 행동을 남기며 아직 실측하지 않은 시간 SLO는 만들지 않는다.

친구 한 명의 첫 관측은 유효한 사용자 피드백이다. 기존 두 명 검토 목표는 남기되 두 번째 사람을 기다리느라 첫 개선을 막지 않는다. 사람 과업의 플랫폼과 실제 수행 항목을 기록하며 Mac RC4-08과 Linux 결과를 합산하지 않는다. 정상 종료 후 시험 자산 보존·관리 방식을 안내하고 미구현 `uninstall`을 과업에 넣지 않는다.

LNX-06은 실제 Linux 환경의 동종 4-validator, 두 정족수 복구, wrong-pin과 후속 거래를 인수한다. 단일 머신의 4개 process와 4개 host의 네트워크를 구분하고 G1-L의 다중 host 조건을 별도로 채운다. 재부팅·장기 운영·성능·백업/복구·업데이트 범위도 각각의 gate가 필요하다.

## 8. WSL2는 후속 검증

실제 Linux의 기능과 기준 package가 안정된 뒤 동일 Linux CLI/runtime을 WSL2에서 검증한다. WSL2는 Windows native 지원을 뜻하지 않는다. Windows build·WSL version/kernel·배포판·CPU·systemd·network mode와 저장 위치를 기록한다.

WSL의 Linux filesystem과 Windows-mounted 경로의 권한·파일 동작을 구분하고, 초기는 Linux filesystem 안에 시험 자료를 둔다. WSL 시작/종료·Windows 재시작, localhost/광고 주소·방화벽, shell/PATH와 파일 전달 경로의 차이를 따로 검사한다. systemd가 동작해도 서비스가 WSL 인스턴스를 계속 살려 두는 것은 아니므로 장기 운영 수명주기를 일반 Linux 서버와 같은 것으로 설명하지 않는다. 근거: [Microsoft WSL systemd 안내](https://learn.microsoft.com/en-us/windows/wsl/systemd).

WSL 준비나 remote Mac 제공을 LNX-01~06의 필수 선행으로 두지 않는다. WSL이 아직 준비되지 않아도 실제 Linux 개발과 친구의 사용자 과업은 계속 진행한다.

## 9. 완료·기록 규칙

- 코드 변경은 `make check`와 변경에 맞는 Linux target 검사를 수행한다. native Linux build/link·CLI 실행·JRE/engine·systemd·사용자 과업은 별도 결과로 남긴다. 실제 관리 CLI 배포 인수는 별도 공급 환경변수 3개와 zsh가 필요한 opt-in 시험이다. 해당 조건이 없어서 SKIP한 사례는 전체 `make check`가 통과해도 실물 배포 PASS로 계산하지 않는다.
- 원자료에는 source commit/dirty, toolchain/Cargo.lock, OS/CPU/kernel/glibc/systemd/JRE, CLI/runtime/JAR hash, 수행 횟수·실패·재시도·정리와 미실행 항목을 기록한다. 공개 결과에는 필요한 identity·정제 요약·digest만 남긴다.
- 실제 Linux가 없는 환경에서 service/native PASS를 만들지 않는다. host probe의 조회 실패·미지원·미검사는 성공으로 합치지 않는다. 지원표의 구현과 인수 상태를 각각 갱신한다.
- G1-L 전체·고객 배포·J5·A7·DB migration·engine upgrade는 이번 기반 코드나 첫 사용자 과업만으로 완료되지 않는다. development·PROPOSED 공급 정책을 유지하며 NIGO 요구사항 상태는 해당 저장소의 현재 원장을 별도로 확인한다.
