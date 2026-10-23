# 초기화 보고서의 private 생성 권한 — CLI05-03-DIAG-01

- 날짜: 2026-10-23
- 상태: **신규 초기화 보고서 생성·수집 개선 검증 완료. 기존 0644 복구는 별도 후속.**
- 대상: 독립 관리 CLI의 새 `init`/`resume-init` 및 이를 호출하는 설치 흐름
- 근거: [CLI05-03의 진단 수집 제한](../results/2026-10-23-cli05-runtime-acceptance.md), [원인 재현·개선 검증](../results/2026-10-23-private-initialization-report.md)

## 확인한 원인

NIGO RC.3의 `EngineProcessReport`는 `FileChannel.open(CREATE_NEW, WRITE)`로 report를 생성하며 POSIX mode를 지정하지 않는다. 생성 mode는 JVM에 상속된 umask의 영향을 받는다. BXDL의 독립 관리 CLI → 고정 실행기 → 일회성 init/resume-init JVM 경로는 umask를 설정하지 않았다. 호출자022에서 report0644가 만들어질 수 있다.

반면 LaunchAgent plist는 기존부터 `Umask=63`(077)을 지정한다. 따라서 서비스 report0600과 init report0644가 같은 control 안에서 공존할 수 있다. 초기화 성공 판정의 입력 reader는0600을 요구하지 않지만 진단 reader는 현재 사용자 소유·단일 link·정확한0600·안전한 부모 경로를 요구한다. 이 차이로 초기화는 성공하고 `initialization.report` 수집은 거절됐다.

기존 관리 CLI의 새 시험을 caller022/077로 대조하고, 개선 CLI를 caller022/002에서 실행해 아래 검증 기준을 확인했다. 과거 CLI05-03 당시의 umask는 기록되지 않았으므로 그 기록을 사후 측정한 값으로 바꾸지 않는다.

## 구현 결정

standalone `src/main.rs`의 첫 동작에서 기존 `rustix` 의존성의 안전한 `process::umask` API로077을 지정한다. 스레드·자식 프로세스·CLI 명령 처리 이전의 한 번뿐인 설정이다. 자식인 고정 RC.3 실행기와 JVM이 이를 상속한다. argv 분류에 의존하지 않아 직접 init/resume-init, setup/기존 console 내부 초기화도 같은 진입 정책을 따른다.

부모 셸이나 다른 프로세스의 umask는 바뀌지 않는다. 라이브러리의 병렬 runner 안에서 임시로 process-wide umask를 변경/복원하지 않는다. `unsafe` 또는 새로운 의존성을 추가하지 않으며 main도 `forbid(unsafe_code)`를 적용한다.

정확한 RC.3 archive·실행기·NIGO/JRE pin과 diagnostic reader는 그대로다. 보고서를 미리 만들면 NIGO의 `CREATE_NEW`와 충돌하므로 사전 생성하지 않는다. 종료 뒤 chmod로 결과를 고치거나 기존 보고서를 읽는 과정에서 권한을 자동 수정하지 않는다.

## 권한과 호환 범위

| 대상 | 정책 |
| --- | --- |
| 새 init/resume-init JVM 생성 파일 | standalone BXDL의077을 상속; report0600 |
| LaunchAgent 서비스 report | 기존 plist077 유지 |
| 노드 설치 payload·manifest | 기존 descriptor 기반 명시 mode 유지: 실행 파일0755·일반 파일0644 |
| 독립 CLI 배포 payload·자동완성·JRE stage | 기존 명시 mode 유지 |
| 준비·catalog·control·진단 export | 기존 private0600/0700 유지 |
| 새 노드 package archive 외부 파일 | standalone CLI의 기본 생성 권한0600; archive 내부 bytes/mode는 유지 |
| 셸이 미리 연 stdout 리다이렉션 | 부모 셸의 생성 정책 적용 |

기존0644 보고서는 수정 CLI로 조회해도 부분 진단으로 남는다. 새 CLI를 설치했다고 과거 자료가 자동 복구됐다고 표시하지 않는다. 이 문제를 없애려는 목적으로 기존 DB를 재초기화하거나 보고서를 삭제하지 않는다. 과거 보고서의 명시적이고 안전한 권한 복구 도구는 별도 후속 범위다.

정책은 standalone 바이너리 진입점에 적용한다. `bxdl::cli::run`이나 공개 Rust 함수를 직접 호출하는 embedding은 process 환경을 스스로 관리하며 이 변경이 process-wide umask를 설정하지 않는다. 또한 기존 RC.3 package 내부의 구형 CLI를 직접 실행하면 그 바이너리의 예전 정책이 적용된다. 지원 profile의 새 초기화에는 개선한 독립 관리 CLI를 사용한다.

## 검증 기준

1. 실제 CLI subprocess의 caller002/022/077에서 native init 자식이077을 상속하고 report0600을 생성한다. 부모 셸 mask는 유지한다. 실패한 stub은 UNKNOWN을 유지하며 자동 재시도하지 않는다.
2. 실제 CLI package build의 외부 archive0600과 동일 내부 bytes를 확인하고 설치 뒤 payload의 명시 mode를 검사한다.
3. 같은 고정 RC.3에 기존CLI022/077과 개선CLI022/002를 각각 새 control·DB로 시험한다. 개선 조건에서는 `logs/diagnose` SUCCEEDED·partial=false·init source COLLECTED 및 INITIALIZED event를 요구한다.
4. 보고서 mode·inode·hash·metadata의 진단 전후/수명주기 불변, payload/JRE/worker 불변과 같은 DB 재시작·정상 종료를 확인한다.
5. 개선 CLI가 기존022 시험의0644 보고서를 조회해도 원본을 바꾸지 않고 부분 상태를 보존하는지 검사한다.
6. 자동 검사·실물 시험·읽기 전용 독립 사후 감사를 구분한다. 성공한 새 init 경로와 이미 존재하는0644 보고서의 미복구 범위를 명시한다. 실제 resume-init 복구·로그아웃·crash·엔진 upgrade·네트워크 전체 재인수는 별도다.
