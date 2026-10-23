# CLI-01 직접 노드 목록·이름 연결 검증 — 2026-10-23

기준 커밋은 `0854dd9`다. 그 위 CLI-01 변경을 검증한다. console의 추가 개발은 중단했으며 기존 공통 catalog/home/target를 재사용했다. NIGO·RC.3·JRE pin·기존 accepted package는 변경하지 않았다.

## 구현 범위

- `nodes`/`nodes add`: 비대화형 직접 조회/이름 연결, 기존 등록/DB/package 불변.
- 카탈로그 V1 무변환 읽기, 명시 연결 V2 저장, 기존 ID/표시명/관측 시각 보존, 중복/자동 rename/동시 갱신/대상 교체 거절.
- 이름/명시 경로 공통 resolver 및 dispatch 전 재검사 API. 실제 서비스 dispatch는 CLI-02에서 연결.
- 과업별 최상위 도움말, 공개 정보만 담은 사람용/JSON 결과.

## 검증 결과

| 검사 | 결과 | 범위 |
| --- | --- | --- |
| `make check` | PASS | Rust 482개·Python 50개, fmt·Clippy |
| 최종 `cargo test --locked --test nodes_cli` | PASS | 13개. 전체 검사 시작 뒤 추가한 구체 이름 오류 회귀 1개를 포함해 이 suite를 다시 실행 |
| 최종 fmt·Clippy | PASS | 마지막 테스트 파일 포함, 경고 없음 |
| Linux `cargo check --locked --target x86_64-unknown-linux-gnu` | PASS | 타입 검사만 수행; Linux 실행/서비스 인수 아님 |
| 실제 Mac PTY | PASS | 아래 10개 검사; 새 debug CLI와 합성 등록 fixture 사용 |
| 문서 링크·fence·`git diff --check` | PASS | 이번 변경 문서/공백 검사 |

최종 트리의 Rust 검증 대상은 총 **483개**다. 전체 검사 시 482개가 통과했고, 뒤에 추가한 테스트 1개는 해당 suite 13개 전체와 최종 fmt/Clippy로 확인했다. 구현 코드는 전체 검사 시작 뒤 변경하지 않았다. 신규 검사는 카탈로그·대상 단위 6개와 CLI 통합 13개다.

실제 Mac PTY 검사:

1. 없는 관리 위치의 빈 목록이 부모도 만들지 않음.
2. `nodes add --help`가 입력·파일 쓰기 없이 종료.
3. 잘못된 이름의 exit 2·`NODE_NAME_INVALID`와 무쓰기.
4. 합성 V1 등록을 기존 `instance show` 계약으로 검증.
5. 사람용 이름 연결 성공과 원본 control 전체 bytes/inode/mode 불변.
6. 같은 이름/대상의 JSON 재요청 성공과 추가 revision 없음.
7. 사람용 목록의 저장 기록·현재 미조회 표시와 무쓰기.
8. JSON 목록의 ID 일치·`NOT_OBSERVED`와 무쓰기.
9. control을 이동한 뒤에도 cached 목록은 대상 접근 없이 반환.
10. 관리 폴더 0700·파일 0600, package/DB/runtime 파일 미생성.

PTY fixture의 package/JRE/DB 참조는 의도적으로 없는 경로였다. 엔진 실행이나 실제 등록 인수의 근거로 사용하지 않는다. 임시 fixture는 종료 시 제거했다. 실행 중 표준입력을 보내지 않았으며 명령이 자체 종료되는 것을 확인했다.

기존 console 회귀 17개도 전체 검사에 포함됐다. 기능 확장 없이 shared catalog의 호환성을 확인한 것이다. 읽기 전용 코드 검토에서는 차단할 결함이 없었고, 잘못된 이름을 일반 인자 오류 대신 규칙을 설명하는 오류로 개선했다.

## 제외한 범위

이름 기반 start/status/stop/logs/diagnose, 순차 setup 문답, 실제 새 package/JVM/launchd/네트워크 인수와 작성자가 아닌 사람의 사용성 과업은 이번 단계에서 실행하지 않았다. 기존 RC.3 package의 인수 결과는 변경 CLI의 근거가 아니다.
