# 현재 구현·검증 상태

기준 소스는 [PR #24](https://github.com/bwg-loonshots/bxdl/pull/24)가 머지된 main `00451dee657013b3ba3a793ddbf864ec66800f58`다. 이 문서는 현재 구현과 확인된 실행 범위를 요약하며, 과거 작업별 기록은 [구현·검증 변경 이력](./implementation-history.md)에 분리한다. 배포본의 정확한 소스·공급물·도구 해시는 그 전달본의 `HANDOFF.json`을 따른다.

BXDL은 Rust 기반 개발 제품이며 **실제 Linux 설치·운용 검증과 직접 CLI의 사용성 평가를 우선**한다. macOS 지정 조합의 검증 결과는 유지한다. `console`은 기존 구현만 보존하고 추가 개발하지 않는다.

## 구현과 인수

| 영역 | 구현 상태 | 확인된 범위·남은 인수 |
| --- | --- | --- |
| 공통 CLI·JSON·zsh 자동완성 | 구현 | macOS·Ubuntu CI 검사 및 CLI native 빌드 통과. Bash/fish 자동완성은 미지원 |
| 패키지 조립·무결성·서명 검증 | 구현 | development manifest v1, 외부 Ed25519 공개키 또는 명시 unsigned 개발 정책. 정식 출시 채널 미제공 |
| JRE·엔진 공급물 | 고정 | macOS Temurin21.0.12.1+1, Linux Temurin21.0.13+9 및 RC.4 조합. Mac RC.3도 지원 유지 |
| 관리 CLI 독립 배포·업데이트·복귀 | 구현 | Mac RC.3·RC.4 지정 조합에서 실행 중 CLI 전환과 노드 보존 확인. Linux 실제 배포·버전 전환 인수는 대기 |
| 순차 준비·조회·수정·설치 | 구현 | Mac archive 기반, Linux flat JRE/JAR/native 설정 기반. 설치 승인과 init/start는 분리 |
| macOS 노드 운용 | 지정 조합 인수 | 사용자 LaunchAgent, 초기화·시작·관측·진단·정상 종료·동일 DB 재시작. 전체 macOS lifecycle·최소 OS·정식 운영 인수는 남음 |
| macOS 네트워크 | 지정 조합 인수 | RC.4 동종 4-validator, 두 복구 경로, wrong-pin, 선정 RPC·거래·관리 CLI 전환. 전체 엔진 기능 인수를 뜻하지 않음 |
| Linux 서비스·권한 | 개발 경로 구현 | systemd, 노드별 비root UID, root 보호 제어 기록, 고정 worker 위임. 실제 노드·계정 격리·서비스 인수는 NOT_RUN |
| Linux 빌드·시험 입력 전달 | 준비 도구 구현 | 고정 source kit·후보 제작/수신·설치 입력 추출·장비 내 fresh 시험 키 생성. 실제 선정 JRE/RC.4 런타임 archive 및 노드 실행은 테스터 과업 |
| 사용자 편의성 평가 | 비공개 과업 준비 | Linux 설치부터 초기화·서비스 운용까지 안내서·결과/개선 양식 동봉. 실제 사용자 관측은 NOT_RUN |
| WSL2·Docker/Compose | 후속 | Linux 실물 기준 조합 검증 뒤 별도 구현·인수 |

Linux의 `bxdl install <archive>` 일반 설치 경로는 제공하지 않는다. 검증한 개발 후보를 준비 도구로 추출한 뒤 `setup new/apply`를 사용한다. Linux의 root 공개 descriptor·고정 worker 위임 구현과 Mac의 승인된 exact runtime profile을 같은 인수 상태로 취급하지 않는다. 자세한 지원 조합은 [지원 범위](./support-matrix.md)를 따른다.

## 자동 검사와 실제 실행 근거

[PR #24 CI](https://github.com/bwg-loonshots/bxdl/actions/runs/44015292246)는 같은 PR source에서 두 플랫폼의 fmt·Clippy·테스트·native CLI 빌드와 `version --json`을 통과했다.

| 환경 | Rust | Python | 의미 |
| --- | --- | --- | --- |
| Ubuntu 24.04 | 605 PASS | 287개 중275 PASS·12 SKIP | Linux CLI compile/link·실행과 자동 회귀. JRE·노드·systemd 인수 아님 |
| macOS 15 CI | 709 PASS | 287개 중285 PASS·2 SKIP | Mac CLI compile/link·실행과 자동 회귀. 아래 실제 엔진 시험과 별도 |

선정 엔진을 실제 실행한 근거는 [RC.4 Mac 단일노드](../results/2026-11-15-rc4-macos-service.md), [RC.4 Mac 네트워크](../results/2026-11-16-rc4-macos-network.md), [기존 RC.3 관리 CLI 조합](../results/2026-10-23-cli05-runtime-acceptance.md)에 있다. 이 결과는 각 문서의 정확한 artifact·환경·시험 범위에 한정한다. CI 통과나 Mac 결과를 Linux 노드 인수로 승계하지 않는다.

## 현재 Linux 시험 범위

대상은 실제 Ubuntu 24.04 LTS·x86_64/glibc·systemd 장비와 sudo 가능한 일반 사용자다. 전달본에는 고정 source·JRE/JAR·준비 도구가 포함되며 빌드는 사전 단계다. 설치 원자료는 검증한 archive에서 준비하고 시험용 키는 장비 안에서 새로 생성한다.

1. 독립 관리 CLI 설치와 최초 관리자 helper 준비.
2. `setup new`의 순차 문답·중단/재개·목록/내용 조회·검토 승인.
3. `setup apply`로 전용 계정·시스템 노드 설치와 이름 연결.
4. 명시 `init` 후 `start/status`의 로컬 READY 확인.
5. `logs/diagnose`, SSH 재접속 후 같은 시도의 지속 동작 확인.
6. 정상 `stop`, 같은 DB에서 새 start와 마지막 정상 종료.

기술 결과와 사용 편의성은 별도로 기록한다. 선행 단계 실패 시 의존 단계는 NOT_RUN으로 남기며, 이미 경험한 문답·오류·도움 요청도 피드백으로 회수한다. 이번 manual peerless 단일 노드 과업은 정족수·거래·재부팅 복구·CLI 버전 전환을 포함하지 않는다.

이번 비공개 Linux 시험의 안내서·빈 결과/개선 양식·개별 실행 기록·전달 archive는 Git 밖에서 관리한다. UAT 폴더를 다시 만들지 않는다. 이 과업에서 저장소에 추가하는 대상은 재사용 가능한 제품/준비 도구와 자동 회귀 검사이며, 기존 `results/`의 검증 근거는 보존한다. 시작점은 [Linux 설치·운용](./linux-operations.md)과 전달본의 `START-HERE.md`다.

## 남은 작업과 우선순위

1. **Linux 실물 시험과 CLI 사용성 개선:** 위 과업을 실행하고 실패·막힘을 수정한다. 선정 JRE/RC.4의 native/RocksDB, 계정 격리, 정상 종료·DB 재시작과 SSH 단절 근거를 확보한다.
2. **Linux 조합 고정과 서버 인수:** 실제 runtime/worker/관리 CLI 바이트를 확정하고 독립 CLI 전환, 장애·재부팅 정책, 동종망·다중 host를 별도로 검증한다. 자동 재시작·부팅 enable은 아직 제공하지 않는다.
3. **공통 운영 기능:** 엔진/DB migration, 안전한 제거, backup/restore·offline Storage A7, UNKNOWN 복구 정책, 전체 시도 조회와 로그 보존·용량 정리.
4. **제품 릴리스 요건:** 전체 dependency NOTICE/SBOM·대응 소스·서명 출처·지원 최소 OS·실패 경계·릴리스 pipeline과 정식 공급 채널을 확정한다.
5. **후속 플랫폼:** WSL2·Docker/Compose와 추가 OS/CPU 조합. Mac의 사용자 세션 종료·sleep/wake·장기 운영 LaunchDaemon도 별도다.

현재 Linux 시험을 시작하기 위해 새 NIGO JAR/API 구현을 선행 요청하지 않는다. 엔진/native 결함이나 계약 차이가 실제로 관측되면 고정된 JAR/JRE/OS identity와 정제 재현 근거로 협의한다. 세부 작업 순서는 [Linux 구현 계획](../design/2026-11-16-linux-first-implementation-plan.md)을 따른다.
