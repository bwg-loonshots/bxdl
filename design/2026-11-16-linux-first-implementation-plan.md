# 실제 Linux 우선 구현·인수 계획

- 결정일: 2026-11-16. 반복 검토자가 실제 Linux 머신을 사용할 수 있으므로 **실제 Linux → Linux 사용자 과업·서버 인수 → WSL2 추가 검증** 순서로 변경한다.
- 상태: **LNX-01 기반 코드 구현·자동 검사 완료. Linux 노드의 일반 설치 UX·실제 머신 인수는 NOT_RUN. LNX-03은 별도 root 승인 spec을 받는 개발용 systemd backend를 구현하며 실제 실행과 구분한다.** [LNX-01 결과](../results/2026-11-16-linux-foundation.md)에 Mac 회귀·Linux target 타입 검사와 실물 지원 여부를 별도로 기록한다.
- 연결: [전체 구현 계획](./2026-09-16-implementation-plan.md), [인수·릴리스 기준](./2026-09-16-acceptance-and-release.md), [CLI 과업](./2026-10-22-integrated-operations-ux-implementation-plan.md), [지원 범위](../docs/support-matrix.md).

## 1. 방향 변경과 보존할 결과

Mac 우선 결정은 운영자가 실제로 사용할 수 있는 개발 환경에서 UX를 검증하기 위한 선택이었다. 이후 Mac RC4-01~06의 지정 조합 기술 인수와 RC4-07 정식 회신 게시를 완료했다. 새 반복 테스터가 실제 Linux를 사용할 수 있으므로 다음 개발과 사용자 피드백의 기준을 Linux로 바꾼다.

Rust, 직접 CLI, 순차 설치 문답, 노드 이름 기반 운용, 관리 CLI와 노드 runtime의 독립 버전 관리는 유지한다. 공통 CLI 개선은 Mac에서도 회귀 검사한다. `bxdl console`의 개발 중단 결정도 유지한다. 자동완성은 현재 구현한 zsh만 유지하며 Linux 지원 추가가 bash/fish 자동완성 지원을 뜻하지 않는다. 실제 테스터의 shell과 안내서 적용 결과를 기록한다.

[RC4-06 결과](../results/2026-11-16-rc4-macos-network.md)·[정제 JSON](../results/2026-11-16-rc4-macos-network.json), Mac runtime R·내부 실행기 A·관리 CLI B/C의 고정 bytes와 [NIGO PR #257](https://github.com/bwg-loonshots/nigo-protocol/pull/257)은 이전 범위의 근거로 보존한다. 새 Linux 구현을 그 당시 결과에 소급하지 않는다. Mac RC4-08/CLI05-04 사람 과업은 **NOT_RUN·우선순위 이연**이다. Linux 사람이 수행한 결과는 Linux profile에 기록하고 Mac 과업을 대신 완료하지 않는다.

## 2. 첫 대상과 환경 확인

| 항목 | 초기 기준·확정 조건 |
| --- | --- |
| 주 검증 환경 | 실제 Linux 머신. 호스트/CPU·배포판·kernel·glibc·systemd를 기록하고 실제 실행 근거를 남김 |
| 사용자 허용 첫 기준 | Ubuntu 24.04 LTS·x86_64/amd64·glibc·systemd·sudo/SSH 가능 조건으로 구현 진행을 허용받음. 실제 host의 배포판/버전·접속 관측은 아직 없으며 다른 배포판·ARM64를 자동 포함하지 않음 |
| Java·저장소 | Java21 full JRE·RocksDB. Linux 공급물·실행 native 조합은 새 lock과 실제 실행으로 선정 |
| 접근·권한 | sudo/SSH 가능은 사용자 답변에 따른 전제다. 실제 접속 대상·권한·OS 정보는 접속 뒤 관측하며 비밀번호·개인키는 문서나 Git에 수집하지 않음 |
| 시험 입력 | 새 test-owned control/data·시험용 signer/TLS·명시 포트. 기존 운영 DB·노드·계정으로 테스트하지 않음 |
| Mac | 기존 지원 조합과 실제 결과 유지, 공통 Rust/CLI 회귀 환경 |
| WSL2 | LNX-07 후속. Linux 실제 머신의 systemd·서버 인수를 대체하지 않음 |
| 이번 범위 밖 | Windows native, Linux ARM64, Alpine/musl, Docker, 기존 DB migration·혼합 버전 rolling upgrade |

사용자가 위 기준으로 진행을 허용했지만 실제 SSH 대상/접속이나 Linux 실행을 완료한 것은 아니다. 2026-11-16 후속 결정으로 이번에는 공급물 수신·도구 준비부터 수행하며 실제 접속 대상은 나중에 정한다. SSH 정보 공유는 필수가 아니다. 테스터가 [실행 안내](../docs/linux-validation.md)대로 도구를 직접 실행하고 정제 결과 JSON만 전달해도 된다. 구현·순수 회귀·Linux target 타입 검사와 실제 실행 대기를 구분한다. Mac에서의 Linux cross-check나 합성 fixture PASS를 Linux linking·배포본 실행·systemd PASS로 보고하지 않는다. 대상 Linux 정보가 달라지면 코드의 임의 허용 범위를 넓히기 전에 지원 후보와 인수 항목을 갱신한다.

## 3. 단계와 완료 기준

| 단계 | 구현·산출물 | 완료 기준 | 현재 상태 |
| --- | --- | --- | --- |
| LNX-01 환경·플랫폼 기반 | Linux runtime inventory staging, Linux 관리 CLI bundle 제작/검증/설치·업데이트 경계, 읽기 전용 host/systemd 준비 상태 검사, 플랫폼별 실패 안내 | Linux와 Mac의 OS/CPU·신뢰·경로 계약 회귀, Linux target 타입 검사, host 검사 무변경·부분 결과 검증. 실제 Linux 실행은 별도 표기 | IMPLEMENTED / AUTO_CHECKED — 실제 Linux 실행 NOT_RUN·노드/서비스 미지원 |
| LNX-02 Linux 고정 공급물 | 선정 Linux JRE 전체 inventory·provenance, 고정 RC.4 JAR의 engine-info/native/RocksDB 사전 실행, Linux runtime R-L·내부 실행기 A-L·관리 CLI C-L의 분리 identity | 실제 Linux에서 JRE/JAR·전체 engine identity·inventory 대조, 고정 package 검증. clean source·archive/manifest/JAR/Java/CLI hash를 기록 | SUPPLY_PINNED / OFFLINE_VERIFIED — 실제 Java/native·Linux CLI build·runtime package NOT_RUN |
| LNX-03 systemd 서비스·권한 | system service adapter, instance별 전용 UID, root 제어 기록, 서비스 시작 gate·시도별 identity, 정상 종료 판정과 정제 로그 | OS 증거와 engine 보고서 결속, 서비스 직접 실행도 gate 적용, 미완료/불명 init 거부, 다른 instance/UID 무영향 및 엔진의 제어 기록 변조 거부 | DEVELOPMENT_BACKEND_AUTO_CHECKED — [검사 결과](../results/2026-11-16-linux-systemd-development.md), 실제 Linux NOT_RUN |
| LNX-04 실제 단일 노드 인수 | Linux package 조립·고정, 고정 실행기 위임, 전용 계정 생성/설치 승인·이름 연결 UX와 전체 bounded 진단 수집 통합, 설치→명시 init→start/status/logs/diagnose/stop, 같은 DB 재시작, SSH 연결 종료 뒤 관측, 실행 중 관리 CLI 전환 | 같은 고정 Linux 조합의 원자료·정제 결과, 정상 정리·payload/worker 불변·DB identity 유지. 단일 노드 로컬 readiness와 전역 합의 구분 | PLAN / NOT_RUN |
| LNX-05 Linux 사용자 과업 | 전달용 package·신뢰 공개키/lock·전용 시험 입력·짧은 안내서·기록 양식. 작성자가 아닌 테스터의 직접 실행 | 처음 설치·준비 수정·중단 후 재개·새 터미널 운용·실패 진단을 관찰. 완료/도움/막힘·시간·문구·실제 명령 기록 | PLAN / NOT_RUN |
| LNX-06 Linux 서버·네트워크 인수 | 동종 4-validator 거래·두 정족수 복구·wrong-pin, systemd lifecycle·재부팅 정책·전용 UID 격리·다중 host | G1-L의 해당 gate별 실제 근거. 로컬4노드와4-host를 구분하고 로그아웃/SSH 단절·재부팅·장애 범위 명시 | PLAN / NOT_RUN |
| LNX-07 WSL2 추가 검증 | 지원 Linux 배포본을 WSL2에서 설치·운용, shell/PATH·filesystem·network·WSL 종료 조건 확인 | Windows/WSL/kernel/배포판/CPU/systemd 조합을 고정한 별도 결과. Linux 서버 PASS를 자동 승계하지 않음 | DEFERRED / NOT_RUN |

LNX-04가 끝나면 LNX-05를 시작할 수 있다. 모든 G1-L 장기·다중 host 검증을 기다린 뒤에만 UX 피드백을 받도록 묶지 않는다. 반대로 초기 사용자 과업 PASS를 LNX-06 완료나 정식 고객 배포로 해석하지 않는다. 각 단계는 실제 구현·자동 검사·host 실행·공급 확인을 별도 상태로 남긴다.

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

LNX-01의 systemd 기반은 **읽기 전용 준비 상태 검사와 명세/파서 검증**이다. unit 생성·등록·daemon-reload·start/stop·사용자 생성·권한 변경·sudo 설정을 자동 수행하지 않는다. 준비 상태 확인 성공은 Linux 서비스 지원 또는 노드 READY가 아니다. LNX-01 당시 서비스 경로는 미지원이었다. LNX-03의 개발용 system service를 별도로 추가하며 이를 user service로 우회하지 않는다.

### LNX-01 읽기 전용 host 검사와 runtime staging

`bxdl host check [--timeout-seconds <1..30>] [--json]`은 host 기준만 읽고 systemd의 system manager를 조회한다. 기본 제한은 5초다. 선택 기준은 Linux GNU/x86_64·Ubuntu 24.04·PID 1 systemd·systemd 255 이상·manager의 `running` 상태다. OS/CPU·배포판·kernel과 WSL 여부, PID 1·manager version/state를 관측한다. 실제 glibc 버전·sudo 권한·JRE/RocksDB·서비스 계정 경계는 이 명령의 검사에 포함하지 않는다.

JSON `data`의 `outcome`/`reasonCode`와 상위 결과의 이유를 구분해 읽는다. 기준을 관측하면 `SUCCEEDED / HOST_BASELINE_OBSERVED`(exit 0), 다른 플랫폼·선정 밖 배포판/버전·PID 1/systemd 상태·WSL은 해당 이유를 가진 `INCOMPLETE`(exit 3)다. 조회 자체의 실패는 오류로 반환한다. WSL은 `HOST_WSL_DEFERRED`로 별도 표기한다. LNX-01 당시 `linuxServiceControl=NOT_IMPLEMENTED`였으며 LNX-03에서는 `DEVELOPMENT_BACKEND_IMPLEMENTED`로 표시한다. `engineAcceptance=NOT_RUN`, `serviceMutations=0`은 유지하므로 host 성공을 서비스/엔진 인수 성공으로 해석하지 않는다. node HTTP 호출·서비스 변경·sudo 요청·파일 생성은 수행하지 않는다.

`runtime stage`는 기존 `darwin/arm64`와 새 `linux/amd64`·`libc=glibc`·Java21 lock을 허용한다. Linux에는 명시 `libc`가 필요하고 Mac lock에는 이 필드를 넣지 않는다. 기존 Mac lock의 형식·bytes를 자동 수정하지 않는다. 이 확장은 offline inventory·안전한 symlink materialization·hash/mode·provenance 검증이며 Linux JRE 선택, archive의 원격 출처 인증 또는 Java 실행을 대신하지 않는다. 실제 Linux runtime lock과 package의 고정은 LNX-02다.

### LNX-01 관리 CLI 배포 구현 경계

Linux 관리 CLI의 초기 target은 `linux-amd64`(Rust `x86_64-unknown-linux-gnu`)다. Linux GNU/x86_64 host에서 실행한 `cli install/update/rollback` 경로를 구현하며, 다른 OS/CPU 배포본의 선택·복귀는 거절한다. Linux ELF64 little-endian·x86_64와 glibc interpreter를 검사하고 Windows/ARM64/musl 지원을 의미하는 capability를 선언하지 않는다. 실제 Linux 빌드·배포·설치 결과는 아직 NOT_RUN이다.

기본 관리 CLI root는 `$HOME/.local/share/BXDL-CLI`, 실행 진입점은 `$HOME/.local/bin/bxdl`이다. `--root`/`--bin-dir`로 명시할 수 있으며 이번에는 XDG 환경변수에 따른 자동 경로 변경을 도입하지 않는다. 이 사용자별 CLI 저장소는 LNX-03의 시스템 노드 설치·root 제어 기록·서비스 UID 저장소와 별개다. 기존 Mac 기본 위치는 유지한다.

공통 노드 목록·준비/실행 기록의 Linux 기본 관리 home은 `$HOME/.local/share/BXDL`이며 `--home`으로 지정할 수 있다. `nodes`·이름 기반 명령·순차 준비가 같은 사용자 catalog를 사용하도록 한다. CLI 배포 저장소인 `BXDL-CLI`와 분리하고 XDG 환경변수를 자동 채택하지 않는다. 이 catalog 기본 경로의 추가는 시스템 노드의 privileged control/data를 사용자 home에 배치한다는 뜻이 아니며, LNX-03에서 설치·운용 권한과 경로를 별도로 인수한다.

Linux 기본 home 적용은 이름 기반 `setup new/resume/list/show/edit`·`nodes` 등의 catalog 경로다. 기존 `bxdl setup`의 생략 `--workspace` 기본값은 아직 Mac 전용이므로 Linux에서는 `--workspace`를 명시한다. Linux 초안의 `dataDirectory` 제안은 입력 기준 디렉터리의 `bxdl-instances/<id>/data`이며 system service의 실제 설치·권한 layout을 확정한 것이 아니다. Linux 서버 control/data 경계는 LNX-03에서 별도로 구현·인수한다.

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

기존 Mac control은 같은 UID를 전제로 하므로 owner 검사를 확장하지 않는다. 별도 Linux control은 새 root0755 디렉터리에 root0600 binding·private journal, root 소유 runtime/JAR/worker/config, 서비스 UID0700 data/output을 둔다. Linux 전용 UID/GID는 경로에서 계산한 `bxdl-<digest>` 계정으로 한정하고, 관리자가 미리 생성한 nologin 계정·전용 그룹을 검증한다. 계정을 암묵 생성하거나 sudoers를 수정하지 않는다.

개발 명령 `linux-service register --instance <새 경로> --spec <json> --allow-development`가 고정 Linux lock·공급 inventory·설정을 검증·보관하고 서비스 UID로 cold 검사를 한다. 이후 `init/resume-init/start/status/stop --instance`와 offline 기록 경로를 연결한다. LNX-03의 명시적인 관리자 경로이며 기존 `setup apply`/`nodes add`를 Linux 시스템 설치 UX로 확장한 것은 아니다. LNX-04에서 package 조립·trust/계정/경로 검토·설치 승인·이름 기반 위임을 연결하고 인수한다.

새 root 소유 gate binary를 시도별 unit이 실행한다. `Type=exec`는 gate 실행 관측이며 Java READY가 아니다. gate는 root 기록을 소비하고 unit/InvocationID/PID/cgroup을 결속한 뒤 고정 `/usr/bin/setpriv`로 보조 그룹·capability를 제거하고 전용 UID/GID로 Java를 exec한다. cold/init도 같은 권한 하향 경계를 사용한다. root journal의 쓰기 descriptor를 엔진에 전달하지 않으며 읽기 전용 operation lock만 stdin으로 상속한다. 자동 restart/enable·범용 shell·임의 Java 옵션은 제공하지 않는다.

종료 직후 `ExecStopPost`가 root 소유 OS 종료 기록을 남겨 systemd GC 이후에도 InvocationID·PID·정상 exit를 검증한다. STOPPED 판정은 이 기록과 engine 보고서의 storage/consensus closure와 root가 결속한 시도의 process/cgroup 부재·DB lock 획득을 함께 요구한다. 종료 불명·gate 중단·권한/자료 불일치를 과거 정상 상태로 덮지 않는다. 실제 Linux에서 계정 격리·잠금·직접 systemctl 재실행·SSH 단절을 확인하기 전에는 systemd 인수 PASS를 기록하지 않는다. [개발 경로와 실제 시험 목록](../docs/linux-service-development.md)을 따른다.

## 6. JRE·엔진 공급과 패키지 고정

NIGO RC.4 source `037c98c315d5322fa7da266ca0254d61f3863622`, JAR SHA-256 `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a`를 새 Linux 조합 검토의 출발점으로 삼는다. 같은 JAR가 Linux에서 적합한지는 Linux JRE·native 실행과 전체 engine-info로 확인한다. 새 JAR 제작을 기본 선행 조건으로 두지 않는다.

Mac용 JRE archive·Java launcher·runtime R은 Linux 공급물이 아니다. Linux JRE의 공급자·patch·CPU·archive/source hash·전체 inventory와 문서/NOTICE를 새 lock으로 확정하고 Linux package를 새로 조립한다. 현재 Mac Temurin 선정은 참고 이력이며 Linux에서의 자동 승인 근거가 아니다.

BXDL은 Linux JRE 선정·패키지 구성·권한·systemd·실제 조합 인수를 담당한다. NIGO에는 선택한 Linux/Java/native/RocksDB 실행 조건과 공급자의 지원·검증 범위를 확인한다. RC.4의 Mac 공급·인수 회신을 Linux 확인 회신으로 확대하지 않는다. 엔진 결함·missing native·계약 불일치가 확인되면 정확한 JAR/JRE/OS identity와 재현 근거로 요청하고 기존 후보 bytes는 보존한다.

Linux runtime R-L/내부 실행기 A-L과 외부 관리 CLI C-L을 별도 고정한다. C-L만 변경하면 같은 R-L을 재사용하되 지원 profile과 영향받는 실제 조합을 검증한다. 초기 Linux 패키지가 아직 고정되지 않은 동안 임의 Linux package의 내부 실행기 위임 capability를 선언하지 않는다. 배포본 검증·CLI 실행 성공과 엔진 지원은 별도다.

NIGO 문서를 수정할 필요가 생기면 전용 worktree와 브랜치를 사용하고 한국어 PR로 공유한다. 현재 Linux 계획 수립만으로 NIGO requirements 수신·동의, 새 engine 후보 또는 JRE 공급자 검증 완료를 기록하지 않는다.

### LNX-02 공급물 선정 결과와 남은 실행

2026-11-16에 Linux용 **Temurin21.0.13+9-LTS full JRE**를 별도로 선정했다. 공식 archive·source·build 자료 수신과 정규화257파일 inventory를 고정하고 Python 준비 도구와 LNX-01 clean CLI의 Rust staging으로 교차 검증했다. 고정 RC.4 JAR/expected identity는 유지하고 Linux Java hash를 새 engine lock에 결합했다. [후보 identity](../packaging/linux-rc4-candidate.json)와 [공급 결과](../results/2026-11-16-linux-rc4-supply.md)를 따른다.

LNX-02 전체 완료를 뜻하지 않는다. 실제 Linux CLI build·Java properties/modules·engine-info는 새 probe로 수행하고, native/RocksDB는 NIGO의 명시 exact-JAR/selected-JRE 시험 경로와 별도 입력·결과로 확인해야 한다. cold probe PASS를 native 실행으로 계산하지 않는다. Linux R-L/A-L/C-L byte identity와 실행기 profile은 실제 Linux build/조립 후 고정하며, 현재 Mac runtime의 pin을 재사용하거나 Linux 서비스 미구현 CLI를 운용 인수된 실행기로 등록하지 않는다.

## 7. 실제 Linux·사용자 검증

LNX-04는 새 설치로 시작한다. 자료 선택·취소/재개·수정·apply 승인, init과 start의 별도 명령, 상태/로그/진단, 정상 stop·같은 DB 재시작, CLI 전환 전후 node/PID/DB/worker 보존을 확인한다. 계정·경로·port·unit 이름은 시험별로 고정하고 기존 서비스를 건드리지 않는다.

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
- G1-L 전체·고객 배포·J5·A7·DB migration·engine upgrade는 이번 기반 코드나 첫 사용자 과업만으로 완료되지 않는다. development·PROPOSED·REQ-0002/0003 OPEN과 기존 공급 정책을 유지한다.
