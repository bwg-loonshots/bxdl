# LNX-04 Linux 설치·운용 UX 자동 검증 — 2026-11-17

결과: **DEVELOPMENT_UX_AUTO_CHECKED**. 실제 Linux 설치·JVM·systemd 실행은 **NOT_RUN**이며 Linux 시스템 변경은 0회다. [정제 JSON](./2026-11-17-linux-operations-ux.json)에 코드 해시·검사 로그 해시·미수행 범위를 기록했다.

기준 커밋은 LNX-03 `b9ee89a46273037ecf832f928606bf86aa446f6f`이고 이번 LNX-04 변경은 미커밋 작업 트리다. Mac arm64에서의 합성 검증과 기존 Mac 회귀, Linux GNU x86_64 타입 검사다. 실제 Linux 바이너리 link·계정 생성·서비스 운용 성공으로 해석하지 않는다.

## 구현과 검증 대상

- 일반 사용자 `setup new/resume/list/show/edit`의 순차 준비·취소/EOF·재개·한 항목 수정·불변 검토 요청. 고정 이름·데이터 위치를 첫 checkpoint에 함께 저장한다.
- 명시 `setup apply` 승인 이후 고정 root helper의 자료 대조·root 사본·전용 계정 생성 의도·노드 등록·완료 확인. root는 사용자 catalog를 쓰지 않는다. 관련 실행은 모의·경계 검사이며 실제 Linux 계정 생성은 수행하지 않았다.
- 공개 root descriptor와 control device/inode·최초 worker hash를 사용자 catalog에 고정한다. 이름 기반 운용은 해당 worker의 제한된 프로토콜로만 위임한다. 관리 CLI 교체가 원본 worker 교체를 뜻하지 않는다.
- 최신 초기화/실행 사건의 제한된 정제 수집과 사용자 `0600` 새 진단 파일 출력. 원문·system journal·전체 과거 이력은 제외하며 누락은 partial이다.
- LNX-03 private state와 새 준비 기록에서 `Option::None`이 `null`로 저장되어 strict JSON 재읽기가 실패하는 문제를 수정했다. 실제 Linux 운영 결과에서 발견한 결함은 아니며 저장→재읽기 합성 회귀로 확인했다.

## 검사 결과

| 검사 | 결과 |
| --- | --- |
| `RUST_TEST_THREADS=4 make check` | PASS — Rust 708 PASS, Python 207개 중 205 PASS·기존 opt-in 2 SKIP, fmt·Clippy PASS |
| 마지막 표시·자동완성 수정 후 fmt·Clippy | PASS |
| 최종 `cargo test --locked --lib linux -- --test-threads=4` | 80 PASS |
| 최종 `cargo test --locked --test completion_cli -- --test-threads=4` | 17 PASS — 전체 회귀 뒤 추가한 Linux 수정 항목 검사 1개 포함 |
| 최종 `cargo build --locked` | Mac 개발 CLI 빌드 PASS |
| 최종 `make check-linux` | Linux GNU x86_64 all-targets 타입 검사 PASS; link/run 아님 |
| 최종 Mac CLI 직접 실행 | 7 PASS — version·빈 Linux 준비 목록·비TTY/JSON 변경 거절·외부 플랫폼 helper/worker 거절·host check·Linux 도움말 |

전체 회귀 실행 중 마지막 상태 표시와 자동완성만 보완했다. 미확정 apply의 엔진 상태를 `NOT_RUN` 대신 `NOT_OBSERVED`로 표시하고, Linux 수정 자동완성에서는 고정 이름/데이터를 제외하며 Mac 기존 항목을 유지했다. 이 변경과 추가 검사는 위 최종 집중 검사·fmt·Clippy·Linux 타입 검사로 확인했다. JSON의 `sourceSha256`은 최종 코드와 테스트를 가리키며, 전체 회귀 708과 최종 집중 검사의 시점을 구분한다.

CLI 직접 실행은 임시 사용자 경로의 부재를 보존했다. Linux 내부 명령은 Mac에서 플랫폼 거절을 확인했으며 sudo·계정 생성·systemd 변경·Linux JVM을 실행하지 않았다. Linux 문답의 정상 재개 등은 테스트 전용 임시 control 경로에서 검사했고, production 경로 `/var/lib/bxdl/instances/<이름>`을 바꾸지 않았다.

## 최초 실패와 재검증

최초 전체 검사에서는 Rust 535 PASS·3 FAIL을 기록했다. 새 timeout 테스트 1개는 자식이 완료 파일을 만들고 내용을 쓰는 사이의 빈 파일을 읽었다. 기존 timeout/no-kill 단언은 유지하고, 같은 제한 시간 안에서 완료 bytes를 관측하도록 수정했다.

기존 Mac application 검사 2개는 병렬 Cargo 재빌드 중 `SETUP_CLI_MISMATCH`로 실패했다. 실행 파일을 교체하는 빌드와 현재 CLI bytes를 대조하는 테스트가 겹친 것으로 판단한다. Mac identity 조건과 테스트 단언을 완화하지 않았으며, 빌드를 분리한 후 전체 회귀에서 모두 통과했다. 최초 로그도 보존했다.

## 남은 실제 Linux 인수

1. 검토한 source로 native CLI를 빌드하고 선정 Linux JRE·RC.4 JAR의 cold/native/RocksDB 실행을 확인한다.
2. Linux runtime archive·원본 실행기·독립 관리 CLI의 bytes와 정식 executor profile을 고정한다. 현재 경로는 root 신뢰에 기반한 개발용이며 정식 runtime 승인은 아니다.
3. 실제 설치·계정 격리·init·systemd 시작/정상 종료·같은 DB 재시작·SSH 단절·실행 중 관리 CLI 전환을 확인한다.
4. 기술 인수한 전달본으로 LNX-05 사용자 과업을 진행한다. WSL2는 후속이다.

현재 사용자 명령·최초 helper 준비·권한은 [Linux 운용 안내](../docs/linux-operations.md), 단계별 범위는 [Linux 구현 계획](../design/2026-11-16-linux-first-implementation-plan.md)을 따른다. 이 단계는 새 NIGO JAR나 API 구현을 선행 요구하지 않는다.
