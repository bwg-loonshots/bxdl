# 지원 환경과 검증 범위

BXDL은 개발용 패키지와 설치·운용 CLI를 제공한다. 현재 개발·실물 검증의 우선 대상은 Linux이며, macOS의 기존 기능과 지정 조합 인수 결과를 유지한다. 아래의 **구현**, **자동 검사**, **실제 노드 인수**는 서로 다른 범위다. 정식 고객 배포 인수가 완료된 제품으로 표시하지 않는다.

## 플랫폼

| 환경 | 제공 기능 | 확인한 범위와 남은 검증 |
| --- | --- | --- |
| Ubuntu 24.04 LTS · x86_64 · glibc · systemd | 독립 관리 CLI, 고정 후보 제작·검증, 순차 setup·설치 승인, 전용 서비스 계정, init/start/status/stop, logs/diagnose | Ubuntu CI의 Rust/Python 검사·native CLI 빌드·version 실행 통과. 선정 JRE/RC.4 runtime 조립과 실제 설치·RocksDB·systemd·SSH 단절·DB 재시작은 비공개 시험 대상이며 아직 NOT_RUN |
| macOS · Apple Silicon arm64 · 사용자 LaunchAgent | 독립 관리 CLI, 패키지 설치, 순차 setup, 초기화·서비스 운용·진단 | 지정 RC.3/RC.4 개발 패키지의 설치·서비스·같은 DB 재시작·4-validator 네트워크와 관리 CLI 전환 인수 결과 보유. 최소 OS 버전·로그아웃/sleep·안전 제거·장기 운영 등 전체 출시 인수는 미완료 |
| WSL2 | 후속 검증 대상 | 현재 Linux 준비 도구는 WSL을 거절하며 실제 Linux 서버 결과를 승계하지 않음 |
| Docker/Compose | 설계상 후속 배포 대상 | 이미지·volume·network·종료/재생성 인수 미구현·미수행 |
| Intel Mac · Linux ARM64 · Windows native · Alpine/musl | 지원 대상 밖 | 해당 OS/CPU용 패키징과 인수 필요 |

Linux 서비스는 PID 1 systemd 255 이상과 일반 사용자·sudo 권한을 전제로 한다. 새 등록·초기화·시작은 system manager가 `running`이어야 한다. macOS 서비스는 로그인된 사용자의 GUI 세션에서 수동 시작하는 LaunchAgent다. LaunchDaemon, 로그인/부팅 자동 시작, 자동 재시작은 제공하지 않는다.

## 엔진·런타임·관리 CLI

| 구성 | 현재 기준 |
| --- | --- |
| NIGO 엔진 | RC.4가 신규 조합의 기준. Mac의 검증된 RC.3 package profile도 관리 CLI에서 유지 |
| Linux JRE | [Temurin 21.0.13+9-LTS · amd64/glibc](../packaging/runtime-lock.temurin-21-linux-amd64.json), 전체 257파일 inventory 및 [Linux RC.4 lock](../packaging/engine-lock.rc4-temurin-linux-development.json) 고정 |
| Mac JRE | [Temurin 21.0.12.1+1-LTS · arm64](../packaging/runtime-lock.temurin-21-macos-arm64.json), [RC.4 lock](../packaging/engine-lock.rc4-temurin-development.json)과 지정 패키지 인수 근거 사용 |
| 저장소 | RocksDB. 기존 DB의 다른 엔진 버전 migration·혼합 버전 rolling upgrade는 제공하지 않음 |
| 패키지 신뢰 | development archive의 전체 inventory/hash·외부 Ed25519 서명 검사. unsigned 개발 공급물은 매회 명시 허용 필요 |
| 관리 CLI 분리 | 관리 CLI는 노드의 고정 실행기·JAR·JRE·DB 밖에서 설치·업데이트·복귀. Linux는 root descriptor와 등록된 worker의 개발용 위임 프로토콜을 사용하며 정식 exact profile 인수는 별도 |

Mac JRE나 실행 파일을 Linux에 사용하지 않는다. Linux manifest의 `minGlibc=2.34`는 후보 제작 때 ELF 요구 버전을 검사하는 계약값이며, Ubuntu 24.04 이외의 모든 glibc 환경을 지원한다는 뜻이 아니다. Mac manifest의 `libc=none`·`minGlibc=none`은 Linux glibc 조건이 적용되지 않는다는 뜻이다.

CLI-only 배포본에는 JRE와 엔진이 없다. 관리 CLI 업데이트만으로 기존 노드 패키지를 다시 만들 필요는 없지만, 실행기·JAR·JRE를 바꾸는 작업은 새 조합 검증이 필요하다. 상세 정책은 [관리 CLI 안내](./management-cli.md), 실제 설치 입력은 [Linux 운용](./linux-operations.md)과 [Mac 준비](./preparation.md)를 따른다.

## 설치·운용의 경계

- Linux 사용자 CLI는 `$HOME/.local/share/BXDL-CLI`, 노드 목록·준비는 `$HOME/.local/share/BXDL`, 실제 시스템 노드·DB는 `/var/lib/bxdl/instances/<이름>` 아래에 둔다. CLI root·bin-dir·관리 home은 별도 옵션이며 XDG 환경변수를 자동 채택하지 않는다.
- Linux는 root 보호 제어 기록과 노드별 비root 실행 계정을 분리한다. 실제 UID 격리·capability·DB lock·systemd 동작은 실물 인수 대상이다. Mac의 CLI와 엔진은 같은 사용자 권한으로 동작하므로 같은 격리를 주장하지 않는다.
- 순차 준비·조회·수정과 `setup apply`, 명시 `init`, `start`는 별도 단계다. 새/빈 데이터만 초기화하며 UNKNOWN 기록을 지우거나 기존 DB를 비워 재시도하지 않는다.
- `status`의 로컬 READY는 전역 정족수·거래 확정과 다르다. `logs/diagnose`는 제한된 저장 기록을 정제하는 기능이며 실시간 health·DB 안전성을 보증하지 않는다. 누락은 partial로 남긴다.
- symlink·hardlink·소유권·모드·경로 충돌과 동시 변경을 검사한다. ACL·mount·파일시스템 내구성, Mac의 보호 폴더 접근, OS 최소 버전의 포괄 지원은 별도 검증 대상이다.
- `upgrade/uninstall`, 자동 백업·복구·키 교체·원격 fleet 관리와 웹 콘솔은 현재 제공 기능에 포함하지 않는다. `bxdl console`은 기존 구현을 보존하며 추가 개발을 중단한 상태다.

## 개발 환경과 검증 근거

Rust 1.86.0과 [Cargo.lock](../Cargo.lock)을 사용한다. `make check`는 fmt·Clippy·Rust 검사와 Python 3 인수 도구 회귀를 수행한다. 첫 개발 빌드에는 C 링커와 Cargo 의존성 확보가 필요하다. 전달된 CLI 사용에는 Rust/Cargo가 필요 없지만, 현재 Linux 비공개 시험 키트는 native 빌드를 사전 단계로 포함한다.

[PR #24의 CI](https://github.com/bwg-loonshots/bxdl/pull/24/checks)는 Ubuntu와 macOS에서 자동 검사와 native CLI 빌드를 통과했다. 이 결과는 JRE·엔진·systemd 실행 시험이 아니다. 각 전달본의 source·도구·공급물은 `HANDOFF.json`과 외부 checksum으로 확인하며, 과거 문서의 commit을 최신 전달본으로 간주하지 않는다.

- [현재 구현 상태](./implementation-status.md): 구현한 기능과 남은 실제 검증.
- [Linux 후보 제작·수신](./linux-native-build.md), [cold 식별 검사](./linux-validation.md): 실제 JRE·엔진 및 설치 시험으로 이어지는 기술 절차.
- [Mac RC.4 서비스 결과](../results/2026-11-15-rc4-macos-service.md), [Mac RC.4 네트워크 결과](../results/2026-11-16-rc4-macos-network.md): 지정 조합의 실제 인수 근거.
- [Linux 실행 계획](../design/2026-11-16-linux-first-implementation-plan.md): 실물 단일노드·사용성·서버/네트워크·WSL2 후속 범위.

비공개 설치·운용 시험의 안내서·결과/개선 의견 양식·개별 실행 기록은 Git 밖에서 전달한다. 아직 수행하지 않은 시험을 코드 구현이나 다른 플랫폼의 결과만으로 PASS로 표시하지 않는다.
