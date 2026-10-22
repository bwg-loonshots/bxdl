# UX-01 공통 문답·항목 수정·상태 안내 검증 — 2026-10-22

## 범위와 기준

- 설계 PR #16 merge `a8fd55a026c812abc6d826e3262bd0e73fd79305` 이후의 UX-01 구현이다.
- [구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)의 공통 입력·한 항목 편집·공개 값 검토·문맥 도움말·상태 안내를 기존 CLI에 연결했다.
- 통합 `bxdl console` 홈·카탈로그는 UX-02 이후다. 웹서버를 추가하지 않았다.
- NIGO source/JAR/JRE/engine lock·기존 archive를 변경하지 않았다. 새 CLI를 기존 설치본에 덮어쓰지 않는다.

## 실행한 검사

| 검사 | 결과 | 확인 범위 |
| --- | --- | --- |
| `make check` | PASS | fmt, Clippy `-D warnings`, Rust 383개, Python 50개 |
| 명령별 도움말 회귀 | PASS | 22개 명령/모드의 `--help`, `-h`, `help <명령>`; stdin 미소비, human/JSON 사용법 일치, 단일 JSON·stderr 무출력 |
| 인자 의미 유지 | PASS | literal `--help` 파일명과 옵션값을 도움말로 가로채지 않음; 잘못된 호출의 기존 오류 계약 |
| 제품·네트워크 문답 | PASS | 도움말/뒤로/취소/EOF, 한 항목 수정 후 검토 복귀, 다른 답 보존, 예약어 파일명은 명시 경로로 사용 |
| 설치 문답·계획 | PASS | 공급 입력 메타 명령 미저장, 무변경/잘못된 값/취소 시 plan·hash·draft 유지, 실제 변경만 계획 무효화 |
| 승인 경계 | PASS | 도움말·뒤로·Enter·취소는 새 출력/apply/init/start 승인 아님; 기존 실행 조건·UNKNOWN 처리 유지 |
| 공개 검토·표시 | PASS | ID·주소·포트 허용목록, 자격 자료 경로 canary 미출력, 환경 기본 경로의 제어문자 정제 |
| 상태 안내 | PASS | 알려진 cold INCOMPLETE, 대기 만료, 실패, 불명, 종료 확인 필요/완료 구분; stale READY로 불명 결과 승격 없음 |
| 실제 PTY 설정 초안 smoke | PASS | 아래 별도 범위 |
| 문서 연결·diff | PASS | 변경 문서 로컬 링크·코드 블록, `git diff --check` |

전체 Rust 결과에는 기존 회귀를 포함한다. 신규 검사는 20개다. focused setup 34개와 CLI 16개는 전체 검사에도 포함되며 별도 개수로 중복 합산하지 않는다. 테스트의 fixture 엔진/서명 package는 실제 NIGO·LaunchAgent 인수와 구분한다.

## 실제 PTY 확인

macOS의 격리된 임시 폴더에서 현재 debug CLI와 예제 제품 설정만 사용했다. `setup --from` 검토에서 도움말을 열고 `edit`의 5번 HTTP 포트를 18181로 수정했다. 6번 이후 필드를 다시 묻지 않고 검토로 돌아왔으며 공개 값에 변경 포트를 표시했다.

출력 경로의 `:help`·`:back`, 출력 승인 화면의 `:help` 뒤 빈 Enter를 시험했다. 승인하지 않은 `instance.json`·예약어 이름의 파일·data 디렉터리는 생성되지 않았다. 초안을 저장한 뒤 명시 비대화형 export로 결과를 읽어 HTTP 포트만 변경되고 nodeId·P2P 설정이 유지됨을 확인했다. 임시 시험 폴더는 종료 후 정리했다.

## 판정과 남은 범위

UX-01 소스 구현과 로컬 검증은 완료했다. JSON envelope·exit code·저장 schema를 변경하지 않았으며 설치·초기화·서비스 자동 실행을 추가하지 않았다.

새 signed archive의 실제 설치·서비스·네트워크 인수, logout/sleep·mutation crash 인수, 작성자가 아닌 사람의 사용성 과업은 이번에 실행하지 않았다. 새 CLI/package 인수는 UX-07이며 기존 RC.3 package PASS를 이번 CLI의 인수 결과로 재사용하지 않는다. UX-02~07·안전 제거·전체 G1-M·J5는 계속 후속이다.
