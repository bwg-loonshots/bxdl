# BXDL Linux 빌드 검증 결과

빈칸은 실제 관측으로 작성한다. 성공·실패와 관계없이 반환한다. 수행하지 않은 단계는 `NOT_RUN`, 환경이나 전달 자료 문제로 시작할 수 없으면 `BLOCKED`, 수행한 단계가 기대 결과와 다르면 `FAIL`로 쓴다. 원인을 아직 모르면 원인 추정과 관측 사실을 나누어 적는다.

## 기본 정보

| 항목 | 작성 |
| --- | --- |
| 테스터 표시 이름 또는 별칭 | |
| 실행 식별자(예: linux-build-01) | |
| 시험 날짜·시간대 | |
| 전달본 source commit (40자리) | |
| 키트 archive SHA-256 | |
| plan SHA-256 | |
| 최초 실행 / 재시도 번호 | |
| 이전 실행과 달라진 조건 | |
| 최종 결과: PASS / FAIL / BLOCKED | |

## 환경과 준비

IP·호스트 주소·계정명·개인 절대 경로·자격증명은 적지 않는다.

| 항목 | 관측값 |
| --- | --- |
| 배포판·버전 (`ID`, `VERSION_ID`) | |
| CPU 아키텍처 (`uname -m`) | |
| 실제 Linux / WSL 여부 | |
| glibc 버전 | |
| 일반 사용자 여부(root 아님) | |
| Python 버전 | |
| rustup / Cargo / rustc 버전·rustc host | |
| C 컴파일러 / readelf 버전 | |
| Cargo 의존성 수신 네트워크 사용 여부 | |
| 추가로 설치하거나 준비한 것 | |
| 환경 준비에 걸린 시간 | |
| 준비 중 받은 도움과 횟수 | |

## 단계별 관측

`소요 시간`에는 기다린 시간과 직접 작업 시간을 구분할 수 있으면 함께 적는다. 오류 문구를 옮길 때 개인 정보는 가린다.

| ID | 과업·기대 결과 | 결과 (PASS/FAIL/BLOCKED/NOT_RUN) | 소요 시간 | 실제 관측·도움·오류 코드 |
| --- | --- | --- | --- | --- |
| ENV-01 | 대상 장비와 도구 조건 확인 | NOT_RUN | | |
| KIT-01 | 별도 받은 값과 전달 archive/plan/source commit 일치 | NOT_RUN | | |
| KIT-02 | 새 폴더에 압축 해제·고정 권한 보존 | NOT_RUN | | |
| BUILD-01 | 빌드 명령 완료, outcome PASS·exit 0 | NOT_RUN | | |
| ART-01 | runtime.tar.gz·management-cli.tar.gz·receipt.json 생성 | NOT_RUN | | |
| RETURN-01 | 양식 작성·반환 파일 checksum·압축 완료 | NOT_RUN | | |

## 빌드와 반환 identity

| 항목 | 관측값 |
| --- | --- |
| 빌드 시작 시각 / 종료 시각 | |
| 빌드 실제 소요 시간 | |
| 종료 코드 | |
| stdout outcome / reasonCode | |
| receipt 생성 여부 | |
| receipt SHA-256 | |
| runtime.tar.gz SHA-256 / bytes (생성 시) | |
| management-cli.tar.gz SHA-256 / bytes (생성 시) | |
| 반환 archive SHA-256 | 별도 전달 메시지에도 기재 |
| 임의로 바꾼 입력·소스·옵션 유무 | 없다 / 있다: 내용 기재 |
| 반환한 파일 목록 | |

반환 archive 자신의 SHA-256은 압축 완료 후 전달 메시지에 적는다. 그 값을 이 양식 안에 다시 넣어 압축하면 archive hash가 바뀌므로 반복 수정하지 않는다.

## 문제 기록 — 문제마다 복사

- 문제 ID:
- 발생 단계 / 재시도 번호:
- 실행한 명령(개인 경로·비밀은 가림):
- 기대한 결과:
- 실제 결과와 종료 코드 / reasonCode:
- 민감 정보를 제거한 오류 문구:
- 다시 수행했을 때 재현 여부 / 조건:
- 진행에 미친 영향: 계속 가능 / 도움 후 가능 / 진행 중단
- 도움받은 내용과 횟수:
- 임시 해결을 했다면 변경 내용:
- 관련 파일·스크린샷 이름(선택):
- 관측 사실과 구분한 원인 추정(선택):

## 이번 과업에서 실행하지 않은 항목

아래는 기본적으로 **NOT_RUN**이다. 빌드 성공을 다음 단계의 성공으로 옮겨 적지 않는다. 실수로 실행한 항목이 있으면 수행한 명령과 범위를 따로 알린다.

| 항목 | 상태 |
| --- | --- |
| JRE 실행·engine-info·native/RocksDB 실제 실행 | NOT_RUN |
| 노드 설치·키/설정 입력·초기화 | NOT_RUN |
| systemd 등록·start/status/logs/diagnose/stop | NOT_RUN |
| SSH 연결 종료 후 노드 유지 | NOT_RUN |
| 동일 DB 재시작·독립 관리 CLI 전환 | NOT_RUN |
| 일반 사용자 설치·운용 UX 평가 | NOT_RUN |

## 정리

- 성공한 범위:
- 막혀서 수행하지 못한 범위:
- 담당자의 확인 또는 도움이 필요한 것:
- 로컬 실패 원자료 보관 여부:
- `IMPROVEMENTS.md` 작성 여부:
