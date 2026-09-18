# 제품 설정 예시

`instance.development.json`은 제안된 BXDL 제품 입력 schema를 보여주는 예시다. 실행 가능한 NIGO 설정·공식 chain description·설치된 instance가 아니다. 예시에는 실제 키·비밀번호·개발 CA·funding이 없으며 참조된 파일도 제공하지 않는다. `192.0.2.10`과 node ID는 교체할 예시 값이다.

참조 경로는 JSON 파일이 있는 디렉터리를 기준으로 계산한다. 환경변수와 `~`를 확장하지 않는다. 제품 입력 검증은 해당 JSON만 읽는다. 엔진 옵션이 없는 Preflight는 참조 파일과 디렉터리의 metadata를 확인하며 credential 내용·DB·WAL을 열거나 디렉터리를 생성하지 않는다. 참조 경로에 symlink가 있으면 거부한다.

키 파일은 owner read/write 또는 owner read/write + instance group read 수준(예: 0600/0640), secret/data 디렉터리는 다른 사용자 접근과 group write가 없는 권한(예: 0700/0750)이 필요하다. Metadata 검사는 서비스 사용자 ownership·ACL·mount 정책·실제 접근 권한 검증을 대신하지 않는다.

NIGO canonical 검증·key identity·DB 무결성·Java/native/platform·systemd 기동은 `NOT_CHECKED`다. 로컬 metadata 검사가 모두 통과해도 preflight 결과는 `INCOMPLETE`이며 ready를 뜻하지 않는다. NIGO YAML rendering, instance 저장, init/start와 secret 생성은 구현하지 않았다.

제품/native 결합 검사는 [엔진 가이드](../../docs/engine.md)를 따른다. 예제의 `validator-keystore.p12`는 교체할 경로 placeholder이며 파일 형식 표기가 아니다. 실제 validator는 NIGO NGVK 형식이고 TLS key/trust store만 PKCS12/JKS다. 결합 검사는 실제 키 자료를 읽으며 기본 metadata-only 검사와 구분한다.
